-- Authoritative Asset and Maintenance source state for one routed holding.
CREATE SCHEMA IF NOT EXISTS erpx_tenant;

CREATE TABLE IF NOT EXISTS erpx_tenant.asset_maintenance_aggregates (
    aggregate_id UUID NOT NULL,
    aggregate_type TEXT NOT NULL CHECK (aggregate_type IN ('asset', 'maintenance_work_order')),
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

CREATE TABLE IF NOT EXISTS erpx_tenant.asset_maintenance_command_idempotency (
    idempotency_key TEXT PRIMARY KEY CHECK (btrim(idempotency_key) <> ''),
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    operation TEXT NOT NULL CHECK (btrim(operation) <> ''),
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    request_hash TEXT NOT NULL CHECK (length(request_hash) = 64),
    response JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Custody history is append-only and remains separately auditable even though
-- the aggregate payload also contains the complete history for rebuilds.
CREATE TABLE IF NOT EXISTS erpx_tenant.asset_custody_transfers (
    asset_id UUID NOT NULL,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    from_scope JSONB NOT NULL,
    to_scope JSONB NOT NULL,
    from_custodian_ref TEXT NOT NULL CHECK (btrim(from_custodian_ref) <> ''),
    to_custodian_ref TEXT NOT NULL CHECK (btrim(to_custodian_ref) <> ''),
    from_location_ref TEXT NOT NULL CHECK (btrim(from_location_ref) <> ''),
    to_location_ref TEXT NOT NULL CHECK (btrim(to_location_ref) <> ''),
    transferred_at TIMESTAMPTZ NOT NULL,
    actor_ref TEXT NOT NULL CHECK (btrim(actor_ref) <> ''),
    reason TEXT NOT NULL CHECK (btrim(reason) <> ''),
    command_id UUID NOT NULL,
    transfer JSONB NOT NULL,
    PRIMARY KEY (asset_id, aggregate_version)
);

-- This is a source-owned handoff. Operations may project it, but cannot use
-- this table as an operational source transaction.
CREATE TABLE IF NOT EXISTS erpx_tenant.asset_operations_capacity_handoffs (
    source_event_id UUID PRIMARY KEY,
    source_id UUID NOT NULL,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    company_id UUID NOT NULL,
    branch_id UUID,
    reference TEXT NOT NULL CHECK (btrim(reference) <> ''),
    source_domain TEXT NOT NULL CHECK (source_domain = 'assets'),
    source_type TEXT NOT NULL CHECK (source_type = 'maintenance_work_order'),
    asset_id UUID NOT NULL,
    downtime_minutes BIGINT NOT NULL CHECK (downtime_minutes > 0),
    occurred_at TIMESTAMPTZ NOT NULL,
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (holding_ref, idempotency_key)
);

CREATE INDEX IF NOT EXISTS asset_maintenance_aggregates_scope_idx
    ON erpx_tenant.asset_maintenance_aggregates
        (holding_ref, company_id, branch_id, aggregate_type, updated_at DESC);
CREATE INDEX IF NOT EXISTS asset_custody_transfers_scope_idx
    ON erpx_tenant.asset_custody_transfers (holding_ref, asset_id, aggregate_version);
CREATE INDEX IF NOT EXISTS asset_capacity_handoffs_scope_idx
    ON erpx_tenant.asset_operations_capacity_handoffs
        (holding_ref, company_id, branch_id, occurred_at DESC);

ALTER TABLE erpx_tenant.asset_maintenance_aggregates ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.asset_maintenance_aggregates FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.asset_maintenance_command_idempotency ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.asset_maintenance_command_idempotency FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.asset_custody_transfers ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.asset_custody_transfers FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.asset_operations_capacity_handoffs ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.asset_operations_capacity_handoffs FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_asset_maintenance_aggregates
    ON erpx_tenant.asset_maintenance_aggregates;
CREATE POLICY tenant_scope_asset_maintenance_aggregates
    ON erpx_tenant.asset_maintenance_aggregates
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_asset_maintenance_idempotency
    ON erpx_tenant.asset_maintenance_command_idempotency;
CREATE POLICY tenant_scope_asset_maintenance_idempotency
    ON erpx_tenant.asset_maintenance_command_idempotency
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_asset_custody_transfers
    ON erpx_tenant.asset_custody_transfers;
CREATE POLICY tenant_scope_asset_custody_transfers
    ON erpx_tenant.asset_custody_transfers
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_asset_capacity_handoffs
    ON erpx_tenant.asset_operations_capacity_handoffs;
CREATE POLICY tenant_scope_asset_capacity_handoffs
    ON erpx_tenant.asset_operations_capacity_handoffs
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

CREATE OR REPLACE FUNCTION erpx_tenant.reject_asset_custody_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'asset custody history is append-only';
END;
$$;

DROP TRIGGER IF EXISTS asset_custody_transfers_immutable
    ON erpx_tenant.asset_custody_transfers;
CREATE TRIGGER asset_custody_transfers_immutable
    BEFORE UPDATE OR DELETE ON erpx_tenant.asset_custody_transfers
    FOR EACH ROW EXECUTE FUNCTION erpx_tenant.reject_asset_custody_mutation();
