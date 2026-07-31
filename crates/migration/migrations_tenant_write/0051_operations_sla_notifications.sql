-- Durable, tenant-scoped Operations SLA notification inbox.
-- The notification is created in the same write transaction as the
-- escalation event. Provider delivery is downstream; this table owns only
-- bounded claim, lease, acknowledgement, and expiry state.
CREATE TABLE IF NOT EXISTS erpx_tenant.operations_sla_notifications (
    notification_key TEXT PRIMARY KEY CHECK (btrim(notification_key) <> '' AND octet_length(notification_key) <= 512),
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> '' AND octet_length(holding_ref) <= 255),
    exception_id UUID NOT NULL,
    exception_version BIGINT NOT NULL CHECK (exception_version > 0),
    recipient_ref TEXT NOT NULL CHECK (btrim(recipient_ref) <> '' AND octet_length(recipient_ref) <= 255),
    kind TEXT NOT NULL CHECK (kind IN ('sla_escalated')),
    not_before TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'claimed', 'delivered', 'expired')),
    claimed_by UUID,
    claim_token UUID,
    claimed_at TIMESTAMPTZ,
    claim_expires_at TIMESTAMPTZ,
    delivered_at TIMESTAMPTZ,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (not_before < expires_at),
    CHECK ((status = 'claimed') = (claimed_by IS NOT NULL AND claim_token IS NOT NULL AND claimed_at IS NOT NULL AND claim_expires_at IS NOT NULL)),
    CHECK ((status = 'delivered') = (delivered_at IS NOT NULL))
);

CREATE INDEX IF NOT EXISTS operations_sla_notifications_due_idx
    ON erpx_tenant.operations_sla_notifications
       (holding_ref, status, not_before, expires_at, notification_key);

ALTER TABLE erpx_tenant.operations_sla_notifications ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.operations_sla_notifications FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_operations_sla_notifications
    ON erpx_tenant.operations_sla_notifications;
CREATE POLICY tenant_scope_operations_sla_notifications
    ON erpx_tenant.operations_sla_notifications
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
