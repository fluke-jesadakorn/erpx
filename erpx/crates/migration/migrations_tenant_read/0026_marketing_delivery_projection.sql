-- Disposable Marketing delivery read model. The write-side dispatch table
-- remains authoritative; workers rebuild these rows from delivery events.
CREATE SCHEMA IF NOT EXISTS erpx_tenant;

CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_delivery_read_rows (
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
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS marketing_delivery_read_due_idx
    ON erpx_tenant.marketing_delivery_read_rows
       (holding_ref, state, next_attempt_at, delivery_id);
CREATE INDEX IF NOT EXISTS marketing_delivery_read_profile_idx
    ON erpx_tenant.marketing_delivery_read_rows
       (holding_ref, profile_id, projected_at DESC, delivery_id);

ALTER TABLE erpx_tenant.marketing_delivery_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_delivery_read_rows FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_marketing_delivery_read_rows
    ON erpx_tenant.marketing_delivery_read_rows;
CREATE POLICY tenant_scope_marketing_delivery_read_rows
    ON erpx_tenant.marketing_delivery_read_rows
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
