
-- Social feed — Facebook-like operations timeline with level-based visibility.
-- Reuses collaboration outbox semantics; rebuildable projection.
-- Extends ADR-017 collaboration; visibility is AuthorityScope.contains() + Audience.includes().

CREATE SCHEMA IF NOT EXISTS erpx_collaboration;

-- Rebuildable feed projection: one row per thread comment (operation).
-- Global feed queries filter by visibility_scope + audience + classification.
CREATE TABLE IF NOT EXISTS erpx_collaboration.feed_items (
    holding_id UUID NOT NULL,
    feed_item_id UUID NOT NULL,
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
    hashtags TEXT[] NOT NULL DEFAULT '{}' CHECK (array_length(hashtags,1) IS NULL OR array_length(hashtags,1) <= 20),
    record_refs JSONB NOT NULL DEFAULT '[]'::JSONB,
    share_of UUID,
    reaction_counts JSONB NOT NULL DEFAULT '{}'::JSONB,
    reply_depth INT NOT NULL DEFAULT 0 CHECK (reply_depth >= 0 AND reply_depth <= 3),
    parent_comment_id UUID,
    PRIMARY KEY (holding_id, feed_item_id),
    FOREIGN KEY (holding_id, thread_id) REFERENCES erpx_collaboration.threads(holding_id, thread_id) ON DELETE RESTRICT,
    CHECK (day_bucket = (created_at AT TIME ZONE 'UTC')::DATE)
);

CREATE INDEX IF NOT EXISTS feed_items_holding_scope_day_created
    ON erpx_collaboration.feed_items (holding_id, day_bucket DESC, created_at DESC, feed_item_id);
CREATE INDEX IF NOT EXISTS feed_items_holding_scope
    ON erpx_collaboration.feed_items (holding_id, visibility_scope);
CREATE INDEX IF NOT EXISTS feed_items_anchor
    ON erpx_collaboration.feed_items (holding_id, anchor_record_id, created_at DESC);
CREATE INDEX IF NOT EXISTS feed_items_author
    ON erpx_collaboration.feed_items (holding_id, author_principal_id, day_bucket DESC, created_at DESC);
CREATE INDEX IF NOT EXISTS feed_items_share_of
    ON erpx_collaboration.feed_items (holding_id, share_of) WHERE share_of IS NOT NULL;

-- Hashtag topics: normalized lower [a-z0-9_], 2..64, cap per item enforced in domain.
CREATE TABLE IF NOT EXISTS erpx_collaboration.hashtag_topics (
    holding_id UUID NOT NULL,
    hashtag_id UUID NOT NULL,
    tag TEXT NOT NULL CHECK (tag ~ '^[a-z0-9_]{2,64}$'),
    created_by_principal_id UUID NOT NULL,
    created_by_kind TEXT NOT NULL CHECK (created_by_kind IN ('human','agent')),
    created_at TIMESTAMPTZ NOT NULL,
    feed_item_count BIGINT NOT NULL DEFAULT 0 CHECK (feed_item_count >= 0),
    PRIMARY KEY (holding_id, hashtag_id),
    UNIQUE (holding_id, tag)
);

CREATE EXTENSION IF NOT EXISTS pg_trgm;
CREATE INDEX IF NOT EXISTS hashtag_topics_tag_trgm
    ON erpx_collaboration.hashtag_topics USING gin (tag gin_trgm_ops);

CREATE TABLE IF NOT EXISTS erpx_collaboration.hashtag_follows (
    holding_id UUID NOT NULL,
    hashtag_id UUID NOT NULL,
    principal_id UUID NOT NULL,
    following BOOLEAN NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, hashtag_id, principal_id),
    FOREIGN KEY (holding_id, hashtag_id) REFERENCES erpx_collaboration.hashtag_topics(holding_id, hashtag_id) ON DELETE CASCADE
);

-- Feed shares are modeled as first-class feed_items with share_of.
-- No separate feed_shares table needed; share_of foreign key self-references logical feed item.
-- Enforce stricter-wins via trigger in application.

ALTER TABLE erpx_collaboration.feed_items ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_collaboration.feed_items FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_collaboration.hashtag_topics ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_collaboration.hashtag_topics FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_collaboration.hashtag_follows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_collaboration.hashtag_follows FORCE ROW LEVEL SECURITY;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM erpx_meta.schema_manifest
        WHERE singleton=TRUE AND schema_revision=2 AND manifest_version=1
          AND database_role='holding_write' AND migration_set='holding-write'
    ) THEN
        RAISE EXCEPTION 'holding-write current manifest is not current';
    END IF;
END;
$$;
