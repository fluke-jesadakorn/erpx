-- Bounded retention and irreversible redaction for form submissions.
-- Submission identity and audit/outbox facts remain durable; submitted values
-- are replaced with an empty object when the retention deadline is reached.
ALTER TABLE erpx_tenant.marketing_content_form_submissions
    ADD COLUMN IF NOT EXISTS retention_until TIMESTAMPTZ NOT NULL
        DEFAULT (now() + INTERVAL '365 days'),
    ADD COLUMN IF NOT EXISTS redacted_at TIMESTAMPTZ;

ALTER TABLE erpx_tenant.marketing_content_form_submissions
    ALTER COLUMN retention_until DROP DEFAULT,
    ADD CONSTRAINT marketing_content_submission_retention_after_received
        CHECK (retention_until >= received_at),
    ADD CONSTRAINT marketing_content_submission_redacted_after_received
        CHECK (redacted_at IS NULL OR redacted_at >= received_at);

CREATE INDEX IF NOT EXISTS marketing_content_submissions_retention_idx
    ON erpx_tenant.marketing_content_form_submissions
        (holding_ref, retention_until, redacted_at, submission_id);
