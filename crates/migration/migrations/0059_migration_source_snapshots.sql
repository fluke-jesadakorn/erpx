-- Durable, payload-bounded source snapshot evidence for the Legacy bridge.
-- This records a read-only source extraction and queues work; it never
-- changes Legacy or ERPX command ownership.

CREATE TABLE IF NOT EXISTS erpx_control.migration_source_snapshots (
    snapshot_id UUID PRIMARY KEY,
    run_id UUID NOT NULL REFERENCES erpx_control.migration_runs(run_id) ON DELETE RESTRICT,
    source_contract_hash TEXT NOT NULL CHECK (source_contract_hash ~ '^[0-9a-fA-F]{64}$'),
    extractor_version TEXT NOT NULL CHECK (length(trim(extractor_version)) BETWEEN 1 AND 128),
    snapshot JSONB NOT NULL,
    missing_table_count BIGINT NOT NULL CHECK (missing_table_count >= 0),
    entity_kind_count BIGINT NOT NULL CHECK (entity_kind_count >= 0),
    total_rows BIGINT NOT NULL CHECK (total_rows >= 0),
    captured_at TIMESTAMPTZ NOT NULL,
    actor_ref TEXT NOT NULL CHECK (length(trim(actor_ref)) BETWEEN 1 AND 200),
    correlation_id UUID NOT NULL,
    idempotency_key TEXT NOT NULL CHECK (length(trim(idempotency_key)) BETWEEN 1 AND 200),
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (octet_length(snapshot::text) <= 16777216),
    UNIQUE (run_id, source_contract_hash),
    UNIQUE (run_id, idempotency_key)
);

CREATE INDEX IF NOT EXISTS migration_source_snapshots_run_idx
    ON erpx_control.migration_source_snapshots (run_id, recorded_at DESC, snapshot_id);
