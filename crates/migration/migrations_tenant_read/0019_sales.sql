-- Sales read state is disposable and rebuilt from the tenant write aggregates
-- and outbox. It is never a command authority.
CREATE SCHEMA IF NOT EXISTS erpx_tenant;

CREATE TABLE IF NOT EXISTS erpx_tenant.sales_aggregates (
    aggregate_id UUID NOT NULL,
    aggregate_type TEXT NOT NULL,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    company_id UUID NOT NULL,
    branch_id UUID,
    reference TEXT NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    payload JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (aggregate_type, aggregate_id)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.sales_projection_checkpoints (
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    last_event_id UUID NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (aggregate_type, aggregate_id)
);

CREATE INDEX IF NOT EXISTS sales_read_aggregates_scope_idx
    ON erpx_tenant.sales_aggregates (holding_ref, company_id, branch_id, aggregate_type, reference);

ALTER TABLE erpx_tenant.sales_aggregates ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.sales_aggregates FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.sales_projection_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.sales_projection_checkpoints FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_sales_read_aggregates ON erpx_tenant.sales_aggregates;
CREATE POLICY tenant_scope_sales_read_aggregates ON erpx_tenant.sales_aggregates
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

DROP POLICY IF EXISTS tenant_scope_sales_projection_checkpoints
    ON erpx_tenant.sales_projection_checkpoints;
CREATE POLICY tenant_scope_sales_projection_checkpoints
    ON erpx_tenant.sales_projection_checkpoints
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
