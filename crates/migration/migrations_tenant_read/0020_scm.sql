-- SCM read state is disposable and is rebuilt from the source outbox.
CREATE SCHEMA IF NOT EXISTS erpx_tenant;

CREATE TABLE IF NOT EXISTS erpx_tenant.scm_aggregates (
    aggregate_id UUID NOT NULL,
    aggregate_type TEXT NOT NULL CHECK (aggregate_type IN (
        'product', 'warehouse', 'purchase_request', 'purchase_order',
        'goods_receipt', 'stock_movement', 'stock_transfer', 'stock_reservation'
    )),
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    company_id UUID NOT NULL,
    branch_id UUID,
    reference TEXT NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    payload JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (aggregate_type, aggregate_id)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.scm_projection_checkpoints (
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    last_event_id UUID NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (aggregate_type, aggregate_id)
);

CREATE INDEX IF NOT EXISTS scm_read_aggregates_scope_idx
    ON erpx_tenant.scm_aggregates
       (holding_ref, company_id, branch_id, aggregate_type, reference);

ALTER TABLE erpx_tenant.scm_aggregates ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.scm_aggregates FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.scm_projection_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.scm_projection_checkpoints FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_scm_read_aggregates ON erpx_tenant.scm_aggregates;
CREATE POLICY tenant_scope_scm_read_aggregates ON erpx_tenant.scm_aggregates
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

DROP POLICY IF EXISTS tenant_scope_scm_projection_checkpoints
    ON erpx_tenant.scm_projection_checkpoints;
CREATE POLICY tenant_scope_scm_projection_checkpoints
    ON erpx_tenant.scm_projection_checkpoints
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
