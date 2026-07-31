-- Procurement and Inventory source aggregates are authoritative here.
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
    reference TEXT NOT NULL CHECK (btrim(reference) <> ''),
    version BIGINT NOT NULL CHECK (version > 0),
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    payload JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (aggregate_type, aggregate_id),
    UNIQUE (holding_ref, aggregate_type, idempotency_key)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.scm_command_idempotency (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    request_hash TEXT NOT NULL CHECK (length(request_hash) = 64),
    response JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, idempotency_key)
);

CREATE INDEX IF NOT EXISTS scm_aggregates_scope_idx
    ON erpx_tenant.scm_aggregates
       (holding_ref, company_id, branch_id, aggregate_type, updated_at DESC);

ALTER TABLE erpx_tenant.scm_aggregates ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.scm_aggregates FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.scm_command_idempotency ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.scm_command_idempotency FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_scm_aggregates ON erpx_tenant.scm_aggregates;
CREATE POLICY tenant_scope_scm_aggregates ON erpx_tenant.scm_aggregates
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

DROP POLICY IF EXISTS tenant_scope_scm_idempotency ON erpx_tenant.scm_command_idempotency;
CREATE POLICY tenant_scope_scm_idempotency ON erpx_tenant.scm_command_idempotency
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
