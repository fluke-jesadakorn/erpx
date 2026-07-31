-- Authenticated provider callbacks are durable tenant-write inbox records.
-- Raw signatures and provider payloads are never stored; only verified typed
-- metadata and the exact-body digest cross this boundary.
CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_provider_callback_inbox (
    inbox_key TEXT PRIMARY KEY CHECK (btrim(inbox_key) <> ''),
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    provider_account_ref TEXT NOT NULL CHECK (btrim(provider_account_ref) <> ''),
    provider_event_id TEXT NOT NULL CHECK (btrim(provider_event_id) <> ''),
    delivery_id UUID NOT NULL,
    provider_message_ref TEXT NOT NULL CHECK (btrim(provider_message_ref) <> ''),
    provider_event_at TIMESTAMPTZ NOT NULL,
    received_at TIMESTAMPTZ NOT NULL,
    correlation_id UUID NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('delivered', 'failed', 'bounced')),
    order_class TEXT NOT NULL CHECK (order_class IN (
        'new', 'in_order', 'out_of_order', 'duplicate', 'conflicting_duplicate'
    )),
    event_fingerprint TEXT NOT NULL CHECK (length(event_fingerprint) = 64),
    raw_body_sha256 TEXT NOT NULL CHECK (length(raw_body_sha256) = 64),
    authentication_scheme TEXT NOT NULL CHECK (btrim(authentication_scheme) <> ''),
    verifier_key_ref TEXT NOT NULL CHECK (btrim(verifier_key_ref) <> ''),
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (holding_ref, provider_account_ref, provider_event_id)
);

CREATE INDEX IF NOT EXISTS marketing_provider_callback_delivery_idx
    ON erpx_tenant.marketing_provider_callback_inbox
       (holding_ref, delivery_id, provider_event_at DESC, provider_event_id DESC);

ALTER TABLE erpx_tenant.marketing_provider_callback_inbox
    ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_provider_callback_inbox
    FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_marketing_provider_callback_inbox
    ON erpx_tenant.marketing_provider_callback_inbox;
CREATE POLICY tenant_scope_marketing_provider_callback_inbox
    ON erpx_tenant.marketing_provider_callback_inbox
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
