-- Add the frozen shared event-envelope columns to the tenant outbox.  Older
-- contexts may still have legacy rows; new source contexts must populate the
-- complete envelope before their events are accepted by their own adapters.
ALTER TABLE erpx_tenant.outbox_events
    ADD COLUMN IF NOT EXISTS contract_version INTEGER,
    ADD COLUMN IF NOT EXISTS event_version INTEGER,
    ADD COLUMN IF NOT EXISTS tenant_scope JSONB,
    ADD COLUMN IF NOT EXISTS actor JSONB,
    ADD COLUMN IF NOT EXISTS correlation_id UUID,
    ADD COLUMN IF NOT EXISTS causation_id UUID,
    ADD COLUMN IF NOT EXISTS idempotency_key TEXT;

ALTER TABLE erpx_tenant.outbox_events
    DROP CONSTRAINT IF EXISTS outbox_events_contract_version_check,
    ADD CONSTRAINT outbox_events_contract_version_check
        CHECK (contract_version IS NULL OR contract_version > 0),
    DROP CONSTRAINT IF EXISTS outbox_events_event_version_check,
    ADD CONSTRAINT outbox_events_event_version_check
        CHECK (event_version IS NULL OR event_version > 0),
    DROP CONSTRAINT IF EXISTS outbox_events_idempotency_key_check,
    ADD CONSTRAINT outbox_events_idempotency_key_check
        CHECK (idempotency_key IS NULL OR (btrim(idempotency_key) <> '' AND octet_length(idempotency_key) <= 512));

CREATE INDEX IF NOT EXISTS outbox_events_envelope_holding_idx
    ON erpx_tenant.outbox_events (holding_ref, occurred_at, event_id);
