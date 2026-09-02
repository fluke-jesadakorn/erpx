
-- Control-plane mirror for global collaboration feed (Platform scope).
-- Mirrors holding feed semantics for global threads.

CREATE TABLE IF NOT EXISTS erpx_global_collaboration.feed_items (
    feed_item_id UUID PRIMARY KEY,
    holding_id UUID NOT NULL,
    thread_id UUID NOT NULL,
    comment_id UUID,
    anchor_record_id UUID NOT NULL,
    anchor_record_ref JSONB NOT NULL,
    author_principal_id UUID NOT NULL,
    author_kind TEXT NOT NULL CHECK (author_kind IN ('human','agent')),
    audience_document JSONB NOT NULL,
    visibility_scope JSONB NOT NULL,
    classification TEXT NOT NULL CHECK (classification IN ('public','internal','confidential','restricted')),
    created_at TIMESTAMPTZ NOT NULL,
    day_bucket DATE NOT NULL,
    hashtags TEXT[] NOT NULL DEFAULT '{}',
    record_refs JSONB NOT NULL DEFAULT '[]'::JSONB,
    share_of UUID,
    reaction_counts JSONB NOT NULL DEFAULT '{}'::JSONB,
    reply_depth INT NOT NULL DEFAULT 0 CHECK (reply_depth >= 0 AND reply_depth <= 3),
    parent_comment_id UUID,
    CHECK (day_bucket = (created_at AT TIME ZONE 'UTC')::DATE)
);

CREATE INDEX IF NOT EXISTS global_feed_items_day_created
    ON erpx_global_collaboration.feed_items (holding_id, day_bucket DESC, created_at DESC, feed_item_id);

CREATE TABLE IF NOT EXISTS erpx_global_collaboration.hashtag_topics (
    hashtag_id UUID PRIMARY KEY,
    holding_id UUID NOT NULL,
    tag TEXT NOT NULL CHECK (tag ~ '^[a-z0-9_]{2,64}$'),
    created_by_principal_id UUID NOT NULL,
    created_by_kind TEXT NOT NULL CHECK (created_by_kind IN ('human','agent')),
    created_at TIMESTAMPTZ NOT NULL,
    feed_item_count BIGINT NOT NULL DEFAULT 0 CHECK (feed_item_count >= 0),
    UNIQUE (holding_id, tag)
);

CREATE TABLE IF NOT EXISTS erpx_global_collaboration.hashtag_follows (
    hashtag_id UUID NOT NULL REFERENCES erpx_global_collaboration.hashtag_topics(hashtag_id) ON DELETE CASCADE,
    principal_id UUID NOT NULL,
    following BOOLEAN NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (hashtag_id, principal_id)
);

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM erpx_meta.schema_manifest
        WHERE singleton=TRUE AND schema_revision=2 AND manifest_version=1
          AND database_role='control_write' AND migration_set='control-write'
    ) THEN
        RAISE EXCEPTION 'control-write current manifest is not current';
    END IF;
END;
$$;
