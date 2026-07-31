-- Durable internal owner-notification handoffs emitted by Marketing journeys.
CREATE SCHEMA IF NOT EXISTS erpx_tenant;

CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_journey_owner_notifications (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    notification_ref UUID NOT NULL,
    journey_id UUID NOT NULL,
    run_id UUID NOT NULL,
    profile_id UUID NOT NULL,
    scope JSONB NOT NULL,
    owner_ref TEXT NOT NULL CHECK (btrim(owner_ref) <> '' AND octet_length(owner_ref) <= 160),
    occurred_at TIMESTAMPTZ NOT NULL,
    version BIGINT NOT NULL CHECK (version = 1),
    payload JSONB NOT NULL,
    payload_hash TEXT NOT NULL CHECK (length(payload_hash) = 64),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, notification_ref),
    UNIQUE (holding_ref, run_id, journey_id, profile_id, notification_ref)
);

CREATE INDEX IF NOT EXISTS marketing_journey_owner_notifications_scope_idx
    ON erpx_tenant.marketing_journey_owner_notifications
       (holding_ref, occurred_at DESC, notification_ref);

ALTER TABLE erpx_tenant.marketing_journey_owner_notifications ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_journey_owner_notifications FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_marketing_journey_owner_notifications
    ON erpx_tenant.marketing_journey_owner_notifications;
CREATE POLICY tenant_scope_marketing_journey_owner_notifications
    ON erpx_tenant.marketing_journey_owner_notifications
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
