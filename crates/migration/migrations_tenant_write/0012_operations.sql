-- Operations is tenant-local. The database route and the transaction-local
-- holding_ref setting are the isolation boundary; no control-plane tables are
-- created by this migration.
CREATE SCHEMA IF NOT EXISTS erpx_tenant;

CREATE TABLE IF NOT EXISTS erpx_tenant.operations_aggregate_snapshots (
    aggregate_type TEXT NOT NULL CHECK (btrim(aggregate_type) <> ''),
    aggregate_id UUID NOT NULL,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    scope JSONB NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    lifecycle_status TEXT NOT NULL CHECK (btrim(lifecycle_status) <> ''),
    state JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (aggregate_type, aggregate_id)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.operations_events (
    event_id UUID PRIMARY KEY,
    contract_version INTEGER NOT NULL CHECK (contract_version > 0),
    event_type TEXT NOT NULL CHECK (btrim(event_type) <> ''),
    event_version INTEGER NOT NULL CHECK (event_version > 0),
    occurred_at TIMESTAMPTZ NOT NULL,
    aggregate_type TEXT NOT NULL CHECK (btrim(aggregate_type) <> ''),
    aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    tenant_scope JSONB NOT NULL,
    actor JSONB NOT NULL,
    correlation_id UUID NOT NULL,
    causation_id UUID,
    idempotency_key TEXT,
    payload JSONB NOT NULL,
    applied_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (aggregate_type, aggregate_id, aggregate_version),
    UNIQUE (idempotency_key)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.operations_source_references (
    aggregate_type TEXT NOT NULL CHECK (btrim(aggregate_type) <> ''),
    aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    source_domain TEXT NOT NULL CHECK (
        btrim(source_domain) <> '' AND lower(source_domain) <> 'operations'
    ),
    source_type TEXT NOT NULL CHECK (btrim(source_type) <> ''),
    source_id UUID NOT NULL,
    source_event_id UUID NOT NULL,
    operations_event_id UUID,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (aggregate_type, aggregate_id, source_event_id),
    UNIQUE (source_domain, source_type, source_id, source_event_id),
    FOREIGN KEY (aggregate_type, aggregate_id)
        REFERENCES erpx_tenant.operations_aggregate_snapshots (aggregate_type, aggregate_id),
    FOREIGN KEY (operations_event_id)
        REFERENCES erpx_tenant.operations_events (event_id)
);

CREATE INDEX IF NOT EXISTS operations_aggregate_snapshots_holding_idx
    ON erpx_tenant.operations_aggregate_snapshots (holding_ref, aggregate_type, updated_at);
CREATE INDEX IF NOT EXISTS operations_events_holding_idx
    ON erpx_tenant.operations_events (holding_ref, occurred_at, event_id);
CREATE INDEX IF NOT EXISTS operations_source_references_holding_idx
    ON erpx_tenant.operations_source_references (holding_ref, source_domain, source_type);

ALTER TABLE erpx_tenant.operations_aggregate_snapshots ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.operations_aggregate_snapshots FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.operations_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.operations_events FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.operations_source_references ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.operations_source_references FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_operations_aggregate_snapshots
    ON erpx_tenant.operations_aggregate_snapshots;
CREATE POLICY tenant_scope_operations_aggregate_snapshots
    ON erpx_tenant.operations_aggregate_snapshots
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

DROP POLICY IF EXISTS tenant_scope_operations_events ON erpx_tenant.operations_events;
CREATE POLICY tenant_scope_operations_events
    ON erpx_tenant.operations_events
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

DROP POLICY IF EXISTS tenant_scope_operations_source_references
    ON erpx_tenant.operations_source_references;
CREATE POLICY tenant_scope_operations_source_references
    ON erpx_tenant.operations_source_references
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
