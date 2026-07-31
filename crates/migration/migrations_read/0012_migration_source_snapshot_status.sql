-- Rebuildable, payload-free source snapshot status projection.

ALTER TABLE erpx_control.migration_status_read_model
    ADD COLUMN IF NOT EXISTS source_snapshot_hash TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS source_missing_table_count BIGINT NOT NULL DEFAULT 0
        CHECK (source_missing_table_count >= 0),
    ADD COLUMN IF NOT EXISTS source_entity_kind_count BIGINT NOT NULL DEFAULT 0
        CHECK (source_entity_kind_count >= 0),
    ADD COLUMN IF NOT EXISTS source_total_rows BIGINT NOT NULL DEFAULT 0
        CHECK (source_total_rows >= 0),
    ADD COLUMN IF NOT EXISTS source_captured_at TIMESTAMPTZ;
