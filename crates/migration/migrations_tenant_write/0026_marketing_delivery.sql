-- Authoritative Marketing delivery dispatches. Provider credentials and
-- provider payloads remain outside this table; the JSON stores only the
-- domain dispatch contract and safe provider references.
CREATE SCHEMA IF NOT EXISTS erpx_tenant;

CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_deliveries (
    delivery_id UUID PRIMARY KEY,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    profile_id UUID NOT NULL,
    campaign_id UUID NOT NULL,
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    state TEXT NOT NULL CHECK (state IN (
        'queued', 'sending', 'retry_scheduled', 'provider_accepted',
        'delivered', 'suppressed', 'failed_terminal'
    )),
    attempt_count INTEGER NOT NULL CHECK (attempt_count >= 0),
    next_attempt_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    version BIGINT NOT NULL CHECK (version > 0),
    payload JSONB NOT NULL,
    claim_id UUID,
    claimed_by UUID,
    claim_until TIMESTAMPTZ,
    last_failure TEXT,
    terminal_reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (holding_ref, idempotency_key)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_delivery_idempotency (
    idempotency_key TEXT PRIMARY KEY CHECK (btrim(idempotency_key) <> ''),
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    delivery_id UUID NOT NULL,
    payload_hash TEXT NOT NULL CHECK (length(payload_hash) = 64),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS marketing_deliveries_due_idx
    ON erpx_tenant.marketing_deliveries
       (holding_ref, state, next_attempt_at, expires_at);
CREATE INDEX IF NOT EXISTS marketing_deliveries_profile_idx
    ON erpx_tenant.marketing_deliveries (holding_ref, profile_id, created_at DESC);

ALTER TABLE erpx_tenant.marketing_deliveries ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_deliveries FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_delivery_idempotency ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_delivery_idempotency FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_marketing_deliveries ON erpx_tenant.marketing_deliveries;
CREATE POLICY tenant_scope_marketing_deliveries ON erpx_tenant.marketing_deliveries
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_marketing_delivery_idempotency
    ON erpx_tenant.marketing_delivery_idempotency;
CREATE POLICY tenant_scope_marketing_delivery_idempotency
    ON erpx_tenant.marketing_delivery_idempotency
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
