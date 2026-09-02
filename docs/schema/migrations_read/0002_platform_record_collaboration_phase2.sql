-- Rebuildable global Collaboration authorization/projection store. Platform
-- reads are permitted only when this checkpoint is current; absence and stale
-- state return no payload, count, or cursor.

CREATE SCHEMA erpx_global_collaboration_read;

CREATE TABLE erpx_global_collaboration_read.projection_state (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    rebuild_generation BIGINT NOT NULL CHECK (rebuild_generation > 0),
    source_high_watermark BIGINT NOT NULL CHECK (source_high_watermark >= 0),
    applied_position BIGINT NOT NULL CHECK (applied_position >= 0),
    state TEXT NOT NULL CHECK (state IN ('ready','rebuilding','unavailable')),
    last_success_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (applied_position <= source_high_watermark),
    CHECK (state <> 'ready' OR applied_position=source_high_watermark)
);

CREATE TABLE erpx_global_collaboration_read.descriptors (
    record_id UUID PRIMARY KEY,
    owner_context TEXT NOT NULL,
    record_type TEXT NOT NULL,
    record_ref JSONB NOT NULL,
    record_scope JSONB NOT NULL CHECK (record_scope=to_jsonb('Platform'::TEXT)),
    authorization_policy_id UUID NOT NULL,
    authorization_policy_version BIGINT NOT NULL CHECK (authorization_policy_version > 0),
    descriptor JSONB,
    canonical_route TEXT,
    display_label TEXT CHECK (display_label IS NULL OR octet_length(display_label) BETWEEN 1 AND 255),
    authorized_search_text TEXT,
    search_vector TSVECTOR GENERATED ALWAYS AS (
        to_tsvector('simple',COALESCE(authorized_search_text,''))
    ) STORED,
    lifecycle_state TEXT,
    classification TEXT,
    descriptor_version BIGINT NOT NULL CHECK (descriptor_version > 0),
    source_event_position BIGINT NOT NULL CHECK (source_event_position > 0),
    tombstoned_at TIMESTAMPTZ,
    projected_at TIMESTAMPTZ NOT NULL,
    CHECK (record_ref ->> 'id'=record_id::TEXT)
);

CREATE INDEX global_collaboration_descriptor_search
    ON erpx_global_collaboration_read.descriptors USING GIN(search_vector)
    WHERE tombstoned_at IS NULL AND display_label IS NOT NULL;

CREATE TABLE erpx_global_collaboration_read.effective_grants (
    record_id UUID NOT NULL REFERENCES erpx_global_collaboration_read.descriptors(record_id),
    principal_id UUID NOT NULL,
    authorization_policy_id UUID NOT NULL,
    policy_version BIGINT NOT NULL CHECK (policy_version > 0),
    can_view BOOLEAN NOT NULL,
    can_relate BOOLEAN NOT NULL,
    effective_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    source_event_position BIGINT NOT NULL CHECK (source_event_position > 0),
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (record_id,principal_id),
    CHECK (expires_at > effective_at)
);

CREATE TABLE erpx_global_collaboration_read.relations (
    relation_id UUID PRIMARY KEY,
    source_record_id UUID NOT NULL,
    target_record_id UUID NOT NULL,
    source_record_ref JSONB NOT NULL,
    target_record_ref JSONB NOT NULL,
    graph_thread_id UUID NOT NULL,
    relation_kind TEXT NOT NULL,
    relation_version BIGINT NOT NULL CHECK (relation_version > 0),
    graph_version BIGINT NOT NULL CHECK (graph_version > 0),
    relation_document JSONB NOT NULL,
    created_by_principal_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    revoked_by_principal_id UUID,
    revoked_at TIMESTAMPTZ,
    committed_sequence BIGINT NOT NULL CHECK (committed_sequence > 0),
    projected_at TIMESTAMPTZ NOT NULL,
    UNIQUE (source_record_id,graph_version),
    CHECK (source_record_ref ->> 'id'=source_record_id::TEXT),
    CHECK (target_record_ref ->> 'id'=target_record_id::TEXT)
);

CREATE INDEX global_collaboration_relation_source
    ON erpx_global_collaboration_read.relations(source_record_id,committed_sequence,relation_id);

CREATE TABLE erpx_global_collaboration_read.thread_projection (
    thread_id UUID PRIMARY KEY,
    anchor_record_id UUID NOT NULL,
    anchor_record_ref JSONB NOT NULL,
    owner_context TEXT NOT NULL,
    record_type TEXT NOT NULL,
    authorization_policy_id UUID NOT NULL,
    classification TEXT NOT NULL,
    visibility_scope JSONB NOT NULL CHECK (visibility_scope=to_jsonb('Platform'::TEXT)),
    audience JSONB NOT NULL,
    retention JSONB NOT NULL,
    projection_document JSONB NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    source_event_position BIGINT NOT NULL CHECK (source_event_position > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    CHECK (anchor_record_ref ->> 'id'=anchor_record_id::TEXT),
    CHECK (updated_at >= created_at)
);

CREATE INDEX global_collaboration_thread_projection_anchor
    ON erpx_global_collaboration_read.thread_projection
       (anchor_record_id,updated_at DESC,thread_id);

CREATE TABLE erpx_global_collaboration_read.comment_projection (
    thread_id UUID NOT NULL REFERENCES erpx_global_collaboration_read.thread_projection(thread_id) ON DELETE CASCADE,
    comment_id UUID NOT NULL,
    projection_document JSONB NOT NULL,
    author_principal_id UUID NOT NULL,
    revision BIGINT NOT NULL CHECK (revision > 0),
    source_event_position BIGINT NOT NULL CHECK (source_event_position > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (thread_id,comment_id),
    CHECK (updated_at >= created_at)
);

CREATE TABLE erpx_global_collaboration_read.reaction_projection (
    thread_id UUID NOT NULL,
    comment_id UUID NOT NULL,
    principal_id UUID NOT NULL,
    reaction TEXT NOT NULL,
    source_event_position BIGINT NOT NULL CHECK (source_event_position > 0),
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (thread_id,comment_id,principal_id,reaction),
    FOREIGN KEY (thread_id,comment_id)
        REFERENCES erpx_global_collaboration_read.comment_projection(thread_id,comment_id)
        ON DELETE CASCADE
);

CREATE TABLE erpx_global_collaboration_read.follower_projection (
    thread_id UUID NOT NULL REFERENCES erpx_global_collaboration_read.thread_projection(thread_id) ON DELETE CASCADE,
    principal_id UUID NOT NULL,
    muted_until TIMESTAMPTZ,
    snoozed_until TIMESTAMPTZ,
    source_event_position BIGINT NOT NULL CHECK (source_event_position > 0),
    followed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (thread_id,principal_id)
);

CREATE TABLE erpx_global_collaboration_read.attachments (
    attachment_id UUID PRIMARY KEY,
    object_id UUID NOT NULL UNIQUE,
    object_key TEXT NOT NULL,
    anchor_record_id UUID NOT NULL,
    anchor_record_ref JSONB NOT NULL,
    thread_id UUID,
    uploader_principal_id UUID NOT NULL,
    classification TEXT NOT NULL,
    file_name TEXT NOT NULL,
    media_type TEXT NOT NULL,
    size_bytes BIGINT NOT NULL CHECK (size_bytes > 0),
    digest BYTEA NOT NULL CHECK (octet_length(digest)=32),
    lifecycle TEXT NOT NULL CHECK (lifecycle IN ('ready','quarantined','expired')),
    lifecycle_version BIGINT NOT NULL CHECK (lifecycle_version > 0),
    source_event_position BIGINT NOT NULL CHECK (source_event_position > 0),
    ready_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,
    projected_at TIMESTAMPTZ NOT NULL,
    CHECK (anchor_record_ref ->> 'id'=anchor_record_id::TEXT),
    CHECK (lifecycle NOT IN ('ready','quarantined') OR thread_id IS NOT NULL),
    CHECK ((lifecycle='ready')=(ready_at IS NOT NULL))
);

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
