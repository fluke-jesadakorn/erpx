-- Reconciliation is identity write-side evidence.  Keep the report durable
-- and append-only so a read model or worker can rebuild it from the source
-- import/outbox facts without trusting a transient in-memory result.
ALTER TABLE erpx_identity.directory_outbox
    ADD COLUMN IF NOT EXISTS event_type TEXT;
ALTER TABLE erpx_identity.directory_outbox
    ADD COLUMN IF NOT EXISTS event_version INTEGER;
ALTER TABLE erpx_identity.directory_outbox
    ADD COLUMN IF NOT EXISTS connector_id UUID;
ALTER TABLE erpx_identity.directory_outbox
    ADD COLUMN IF NOT EXISTS aggregate_version BIGINT;
ALTER TABLE erpx_identity.directory_outbox
    ADD COLUMN IF NOT EXISTS scope_json JSONB;
ALTER TABLE erpx_identity.directory_outbox
    ADD COLUMN IF NOT EXISTS actor_json JSONB;
ALTER TABLE erpx_identity.directory_outbox
    ADD COLUMN IF NOT EXISTS idempotency_key TEXT;

UPDATE erpx_identity.directory_outbox
SET event_type = COALESCE(NULLIF(payload_json->>'event', ''), 'directory.legacy_event.v1'),
    event_version = COALESCE(event_version, 1),
    aggregate_version = COALESCE(aggregate_version, 1),
    scope_json = COALESCE(scope_json, '{}'::jsonb),
    idempotency_key = COALESCE(idempotency_key, event_id::text)
WHERE event_type IS NULL
   OR event_version IS NULL
   OR aggregate_version IS NULL
   OR scope_json IS NULL
   OR idempotency_key IS NULL;

ALTER TABLE erpx_identity.directory_outbox
    ALTER COLUMN event_type SET NOT NULL,
    ALTER COLUMN event_type SET DEFAULT 'directory.legacy_event.v1',
    ALTER COLUMN event_version SET NOT NULL,
    ALTER COLUMN event_version SET DEFAULT 1,
    ALTER COLUMN aggregate_version SET NOT NULL,
    ALTER COLUMN aggregate_version SET DEFAULT 1,
    ALTER COLUMN scope_json SET NOT NULL,
    ALTER COLUMN scope_json SET DEFAULT '{}'::jsonb,
    ALTER COLUMN idempotency_key DROP NOT NULL;

CREATE INDEX IF NOT EXISTS directory_outbox_aggregate_idx
    ON erpx_identity.directory_outbox (aggregate_type, aggregate_id, aggregate_version, occurred_at);

CREATE TABLE IF NOT EXISTS erpx_identity.directory_reconciliation_runs (
    id UUID PRIMARY KEY,
    import_id UUID NOT NULL REFERENCES erpx_identity.directory_imports(id),
    expected_users BIGINT NOT NULL CHECK (expected_users >= 0),
    staged_users BIGINT NOT NULL CHECK (staged_users >= 0),
    canonical_users BIGINT NOT NULL CHECK (canonical_users >= 0),
    expected_digest BYTEA NOT NULL CHECK (octet_length(expected_digest) = 32),
    observed_digest BYTEA NOT NULL CHECK (octet_length(observed_digest) = 32),
    status TEXT NOT NULL CHECK (status IN ('reconciled', 'blocked')),
    checked_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS directory_reconciliation_runs_import_idx
    ON erpx_identity.directory_reconciliation_runs (import_id, checked_at DESC);
