-- Durable checkpoints for rebuilding the disposable Finance journal read
-- model from the tenant write outbox.
CREATE SCHEMA IF NOT EXISTS erpx_tenant;

CREATE TABLE IF NOT EXISTS erpx_tenant.finance_projection_checkpoints (
    journal_id UUID PRIMARY KEY,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    last_event_id UUID NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS finance_projection_checkpoints_scope_idx
    ON erpx_tenant.finance_projection_checkpoints
        (holding_ref, source_version, journal_id);

ALTER TABLE erpx_tenant.finance_projection_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_projection_checkpoints FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_finance_projection_checkpoints
    ON erpx_tenant.finance_projection_checkpoints;
CREATE POLICY tenant_scope_finance_projection_checkpoints
    ON erpx_tenant.finance_projection_checkpoints
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
