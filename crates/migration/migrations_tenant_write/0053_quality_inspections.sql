-- Authoritative Quality inspection aggregates and immutable source events.
CREATE SCHEMA IF NOT EXISTS erpx_tenant;

CREATE TABLE IF NOT EXISTS erpx_tenant.quality_inspections (
    inspection_id UUID PRIMARY KEY,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    company_id UUID NOT NULL,
    branch_id UUID,
    reference TEXT NOT NULL CHECK (btrim(reference) <> ''),
    version BIGINT NOT NULL CHECK (version > 0),
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    payload JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE (holding_ref, idempotency_key)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.quality_command_idempotency (
    idempotency_key TEXT PRIMARY KEY CHECK (btrim(idempotency_key) <> ''),
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    request_hash TEXT NOT NULL CHECK (length(request_hash) = 64),
    response JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS quality_inspections_scope_idx
    ON erpx_tenant.quality_inspections
       (holding_ref, company_id, branch_id, updated_at DESC, inspection_id);

ALTER TABLE erpx_tenant.quality_inspections ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.quality_inspections FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.quality_command_idempotency ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.quality_command_idempotency FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_quality_inspections
    ON erpx_tenant.quality_inspections;
CREATE POLICY tenant_scope_quality_inspections
    ON erpx_tenant.quality_inspections
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_quality_idempotency
    ON erpx_tenant.quality_command_idempotency;
CREATE POLICY tenant_scope_quality_idempotency
    ON erpx_tenant.quality_command_idempotency
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
