ALTER TABLE erpx_control.stripe_event_inbox
    ADD COLUMN IF NOT EXISTS raw_body_sha256 BYTEA,
    ADD COLUMN IF NOT EXISTS correlation_id UUID;

ALTER TABLE erpx_control.stripe_event_inbox
    DROP CONSTRAINT IF EXISTS stripe_event_inbox_raw_body_sha256_length;

ALTER TABLE erpx_control.stripe_event_inbox
    ADD CONSTRAINT stripe_event_inbox_raw_body_sha256_length
    CHECK (raw_body_sha256 IS NULL OR octet_length(raw_body_sha256) = 32);

CREATE INDEX IF NOT EXISTS stripe_event_inbox_correlation_idx
    ON erpx_control.stripe_event_inbox (correlation_id, received_at);
