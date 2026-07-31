-- Read projection metadata follows source redaction without copying any
-- token, secret, or deleted submission value into a new durable fact.
ALTER TABLE erpx_tenant.marketing_content_submission_read_rows
    ADD COLUMN IF NOT EXISTS retention_until TIMESTAMPTZ NOT NULL
        DEFAULT (now() + INTERVAL '365 days'),
    ADD COLUMN IF NOT EXISTS redacted_at TIMESTAMPTZ;

ALTER TABLE erpx_tenant.marketing_content_submission_read_rows
    ALTER COLUMN retention_until DROP DEFAULT,
    ADD CONSTRAINT marketing_content_read_submission_retention_after_received
        CHECK (retention_until >= received_at),
    ADD CONSTRAINT marketing_content_read_submission_redacted_after_received
        CHECK (redacted_at IS NULL OR redacted_at >= received_at);
