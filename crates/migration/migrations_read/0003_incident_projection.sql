CREATE SCHEMA IF NOT EXISTS erpx_control;

-- Rebuildable, safe incident summaries. Internal timeline, broadcasts,
-- mitigation, root cause, and corrective-action content stays on the write
-- side until a separate disclosure policy exists.
CREATE TABLE IF NOT EXISTS erpx_control.incidents (
    id UUID PRIMARY KEY,
    status TEXT NOT NULL CHECK (
        status IN ('investigating', 'identified', 'monitoring', 'resolved', 'postmortem_complete')
    ),
    commander_ref TEXT NOT NULL CHECK (btrim(commander_ref) <> ''),
    leads JSONB NOT NULL CHECK (jsonb_typeof(leads) = 'array'),
    affected_services JSONB NOT NULL CHECK (jsonb_typeof(affected_services) = 'array'),
    affected_customer_accounts JSONB NOT NULL CHECK (jsonb_typeof(affected_customer_accounts) = 'array'),
    version BIGINT NOT NULL CHECK (version > 0),
    payload JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS incidents_status_updated_idx
    ON erpx_control.incidents (status, updated_at DESC, id DESC);
