ALTER TABLE erpx_control.idempotency_keys
    ADD COLUMN IF NOT EXISTS aggregate_type TEXT,
    ADD COLUMN IF NOT EXISTS aggregate_id UUID,
    ADD COLUMN IF NOT EXISTS aggregate_version BIGINT,
    ADD COLUMN IF NOT EXISTS event_id UUID;

CREATE TABLE IF NOT EXISTS erpx_control.projection_event_receipts (
    projection_name TEXT NOT NULL,
    event_id UUID NOT NULL,
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    claimed_by UUID,
    claimed_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    PRIMARY KEY (projection_name, event_id)
);

CREATE INDEX IF NOT EXISTS projection_event_receipts_claim_idx
    ON erpx_control.projection_event_receipts (projection_name, claimed_at)
    WHERE completed_at IS NULL;

CREATE TABLE IF NOT EXISTS erpx_control.tenant_registry (
    tenant_id TEXT PRIMARY KEY,
    holding_ref TEXT NOT NULL UNIQUE,
    write_database_key TEXT NOT NULL,
    read_database_key TEXT NOT NULL,
    storage_namespace TEXT NOT NULL,
    credential_key_ref TEXT NOT NULL,
    backup_chain_ref TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('provisioning', 'active', 'suspended', 'recovery')),
    registry_version BIGINT NOT NULL CHECK (registry_version > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS tenant_registry_state_idx
    ON erpx_control.tenant_registry (state, updated_at);
