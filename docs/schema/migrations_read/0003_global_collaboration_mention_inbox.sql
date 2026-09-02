-- Rebuildable opaque Platform mention inbox. It intentionally contains no
-- record type, title, business identifier, comment body, or recipient count.

CREATE TABLE erpx_global_collaboration_read.opaque_mention_inbox_projection (
    inbox_cursor BIGSERIAL PRIMARY KEY,
    recipient_principal_id UUID NOT NULL,
    event_id UUID NOT NULL,
    thread_id UUID NOT NULL
        REFERENCES erpx_global_collaboration_read.thread_projection(thread_id) ON DELETE CASCADE,
    mention_event_id UUID NOT NULL,
    anchor_record_id UUID NOT NULL,
    source_event_position BIGINT NOT NULL CHECK (source_event_position > 0),
    created_at TIMESTAMPTZ NOT NULL,
    UNIQUE (recipient_principal_id,mention_event_id),
    UNIQUE (event_id)
);

CREATE INDEX global_collaboration_mention_inbox_cursor
    ON erpx_global_collaboration_read.opaque_mention_inbox_projection
       (recipient_principal_id,inbox_cursor);

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM erpx_meta.schema_manifest
        WHERE singleton=TRUE AND schema_revision=2 AND manifest_version=1
          AND database_role='control_read' AND migration_set='control-read'
    ) THEN
        RAISE EXCEPTION 'control-read current manifest is not current';
    END IF;
END;
$$;
