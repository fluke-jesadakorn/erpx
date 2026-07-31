-- Rebuildable Marketing content projection. It stores bounded metadata and
-- opaque references only; authoritative content remains in tenant-write.
CREATE SCHEMA IF NOT EXISTS erpx_tenant;

CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_content_read_rows (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> '' AND octet_length(holding_ref) <= 255),
    aggregate_type TEXT NOT NULL CHECK (aggregate_type IN ('asset', 'template', 'form', 'domain')),
    aggregate_id UUID NOT NULL,
    scope JSONB NOT NULL,
    slug TEXT,
    status TEXT NOT NULL CHECK (btrim(status) <> '' AND octet_length(status) <= 64),
    state JSONB NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    source_event_id UUID NOT NULL,
    source_actor JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, aggregate_type, aggregate_id)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_content_projection_checkpoints (
    holding_ref TEXT NOT NULL,
    projection_name TEXT NOT NULL CHECK (projection_name = 'marketing_content'),
    aggregate_type TEXT NOT NULL CHECK (aggregate_type IN ('asset', 'template', 'form', 'domain')),
    aggregate_id UUID NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    last_event_id UUID NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, projection_name, aggregate_type, aggregate_id)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_content_projection_event_receipts (
    holding_ref TEXT NOT NULL,
    projection_name TEXT NOT NULL CHECK (projection_name = 'marketing_content'),
    event_id UUID NOT NULL,
    aggregate_type TEXT NOT NULL CHECK (aggregate_type IN ('asset', 'template', 'form', 'domain')),
    aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    received_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    processed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, projection_name, event_id)
);

CREATE INDEX IF NOT EXISTS marketing_content_read_scope_idx
    ON erpx_tenant.marketing_content_read_rows (holding_ref, aggregate_type, status, projected_at DESC);
CREATE INDEX IF NOT EXISTS marketing_content_read_source_idx
    ON erpx_tenant.marketing_content_read_rows (holding_ref, aggregate_type, source_version, projected_at DESC);

ALTER TABLE erpx_tenant.marketing_content_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_content_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_content_projection_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_content_projection_checkpoints FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_content_projection_event_receipts ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_content_projection_event_receipts FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_marketing_content_read_rows ON erpx_tenant.marketing_content_read_rows;
CREATE POLICY tenant_scope_marketing_content_read_rows
    ON erpx_tenant.marketing_content_read_rows
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_marketing_content_projection_checkpoints ON erpx_tenant.marketing_content_projection_checkpoints;
CREATE POLICY tenant_scope_marketing_content_projection_checkpoints
    ON erpx_tenant.marketing_content_projection_checkpoints
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_marketing_content_projection_event_receipts ON erpx_tenant.marketing_content_projection_event_receipts;
CREATE POLICY tenant_scope_marketing_content_projection_event_receipts
    ON erpx_tenant.marketing_content_projection_event_receipts
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
