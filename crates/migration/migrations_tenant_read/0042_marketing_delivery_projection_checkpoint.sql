-- Durable cursor for the Marketing delivery projection. The write-side
-- outbox remains authoritative; replaying a committed source event is safe
-- because the read row is version fenced.
CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_delivery_projection_checkpoints (
    projection_name TEXT PRIMARY KEY CHECK (projection_name = 'marketing_delivery'),
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    last_event_id UUID,
    last_occurred_at TIMESTAMPTZ,
    source_event_count BIGINT NOT NULL DEFAULT 0 CHECK (source_event_count >= 0),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE erpx_tenant.marketing_delivery_projection_checkpoints
    ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_delivery_projection_checkpoints
    FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_marketing_delivery_projection_checkpoints
    ON erpx_tenant.marketing_delivery_projection_checkpoints;
CREATE POLICY tenant_scope_marketing_delivery_projection_checkpoints
    ON erpx_tenant.marketing_delivery_projection_checkpoints
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
