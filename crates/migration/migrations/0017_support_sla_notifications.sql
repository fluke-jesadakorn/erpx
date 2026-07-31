-- Durable, replay-safe support SLA notification inbox. Delivery providers are
-- downstream; this table only owns the notification claim and lease state.
CREATE TABLE IF NOT EXISTS erpx_control.support_sla_notifications (
    notification_key TEXT PRIMARY KEY CHECK (btrim(notification_key) <> ''),
    ticket_id UUID NOT NULL REFERENCES erpx_control.support_tickets(id) ON DELETE RESTRICT,
    customer_account_id UUID NOT NULL,
    classification TEXT NOT NULL CHECK (classification IN ('deadline_approaching', 'deadline_breached')),
    audience TEXT NOT NULL CHECK (audience IN ('customer_visible', 'internal')),
    deadline TIMESTAMPTZ NOT NULL,
    not_before TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'claimed', 'delivered', 'expired')),
    claimed_at TIMESTAMPTZ,
    claim_expires_at TIMESTAMPTZ,
    delivered_at TIMESTAMPTZ,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (not_before < expires_at),
    CHECK (claim_expires_at IS NULL OR claimed_at IS NOT NULL),
    CHECK ((status = 'claimed') = (claimed_at IS NOT NULL AND claim_expires_at IS NOT NULL)),
    CHECK ((status = 'delivered') = (delivered_at IS NOT NULL))
);

CREATE INDEX IF NOT EXISTS support_sla_notifications_due_idx
    ON erpx_control.support_sla_notifications
       (status, not_before, expires_at, notification_key);
