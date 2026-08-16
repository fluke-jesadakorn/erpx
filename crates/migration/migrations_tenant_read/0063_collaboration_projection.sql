-- Rebuildable Collaboration projections. Authoritative comment revisions and
-- moderation evidence remain in the anchor tenant-write data plane.

CREATE SCHEMA erpx_collaboration_read;

CREATE FUNCTION erpx_collaboration_read.scope_kind(scope JSONB)
RETURNS TEXT
LANGUAGE sql
IMMUTABLE
STRICT
AS $$
    SELECT CASE
        WHEN jsonb_typeof(scope) = 'string' THEN trim(both '"' from scope::text)
        ELSE (SELECT key FROM jsonb_each(scope) LIMIT 1)
    END
$$;

CREATE FUNCTION erpx_collaboration_read.scope_holding(scope JSONB)
RETURNS TEXT
LANGUAGE sql
IMMUTABLE
STRICT
AS $$
    SELECT COALESCE(
        scope #>> '{Holding,holding_id}',
        scope #>> '{Company,holding_id}',
        scope #>> '{Branch,holding_id}',
        scope #>> '{Subtree,holding_id}'
    )
$$;

CREATE FUNCTION erpx_collaboration_read.scope_contains(authority JSONB, target JSONB)
RETURNS BOOLEAN
LANGUAGE plpgsql
IMMUTABLE
STRICT
AS $$
DECLARE
    authority_kind TEXT := erpx_collaboration_read.scope_kind(authority);
    target_kind TEXT := erpx_collaboration_read.scope_kind(target);
    authority_holding TEXT := erpx_collaboration_read.scope_holding(authority);
    target_holding TEXT := erpx_collaboration_read.scope_holding(target);
    target_company TEXT := COALESCE(
        target #>> '{Company,company_id}', target #>> '{Branch,company_id}'
    );
    target_branch TEXT := target #>> '{Branch,branch_id}';
BEGIN
    IF authority_kind = 'Platform' THEN
        RETURN TRUE;
    END IF;
    IF authority_holding IS NULL OR authority_holding <> target_holding THEN
        RETURN FALSE;
    END IF;
    IF authority_kind = 'Holding' THEN
        RETURN target_kind IN ('Holding', 'Company', 'Branch', 'Subtree');
    END IF;
    IF authority_kind = 'Company' THEN
        RETURN target_kind IN ('Company', 'Branch')
            AND authority #>> '{Company,company_id}' = target_company;
    END IF;
    IF authority_kind = 'Branch' THEN
        RETURN target_kind = 'Branch'
            AND authority #>> '{Branch,company_id}' = target_company
            AND authority #>> '{Branch,branch_id}' = target_branch;
    END IF;
    IF authority_kind = 'Subtree' THEN
        RETURN (
            target_kind = 'Company'
            AND (authority #> '{Subtree,company_ids}') ? target_company
        ) OR (
            target_kind = 'Branch'
            AND (authority #> '{Subtree,branch_ids}') ? target_branch
        );
    END IF;
    RETURN FALSE;
END;
$$;

CREATE FUNCTION erpx_collaboration_read.audience_allows(
    audience JSONB,
    principal_id UUID,
    authority_scope JSONB,
    actor_realm TEXT,
    actor_portal TEXT
)
RETURNS BOOLEAN
LANGUAGE plpgsql
IMMUTABLE
AS $$
DECLARE
    realm TEXT := audience ->> 'realm';
BEGIN
    IF realm = 'internal' THEN
        IF actor_realm <> 'internal' OR actor_portal IS NOT NULL THEN
            RETURN FALSE;
        END IF;
        RETURN erpx_collaboration_read.scope_contains(
            authority_scope,
            audience -> 'scope'
        ) AND (
            jsonb_array_length(audience -> 'principals') = 0
            OR EXISTS (
                SELECT 1
                FROM jsonb_array_elements(audience -> 'principals') AS member
                WHERE member ->> 'id' = principal_id::text
            )
        );
    END IF;
    IF realm = 'external_portal' THEN
        RETURN actor_realm = 'external_portal'
        AND actor_portal IS NOT NULL
        AND audience ->> 'portal' = actor_portal
        AND EXISTS (
            SELECT 1
            FROM jsonb_array_elements(audience -> 'participants') AS member
            WHERE member ->> 'id' = principal_id::text
        );
    END IF;
    RETURN FALSE;
END;
$$;

CREATE TABLE erpx_collaboration_read.thread_projection (
    tenant_id UUID NOT NULL,
    projection_name TEXT NOT NULL DEFAULT 'collaboration.threads'
        CHECK (projection_name = 'collaboration.threads'),
    generation UUID NOT NULL,
    thread_id UUID NOT NULL,
    anchor_record_id UUID NOT NULL,
    visibility_scope JSONB NOT NULL,
    audience JSONB NOT NULL,
    projection JSONB NOT NULL,
    search_text TEXT NOT NULL,
    search_vector TSVECTOR GENERATED ALWAYS AS (
        to_tsvector('simple', search_text)
    ) STORED,
    source_event_cursor BIGINT NOT NULL CHECK (source_event_cursor > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, thread_id),
    FOREIGN KEY (tenant_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (tenant_id, projection_name, generation)
        ON DELETE CASCADE
);

CREATE INDEX collaboration_thread_anchor_projection
    ON erpx_collaboration_read.thread_projection
       (tenant_id, generation, anchor_record_id, updated_at DESC, thread_id);
CREATE INDEX collaboration_thread_search
    ON erpx_collaboration_read.thread_projection USING GIN (search_vector);

CREATE TABLE erpx_collaboration_read.stream_projection (
    tenant_id UUID NOT NULL,
    projection_name TEXT NOT NULL DEFAULT 'collaboration.stream'
        CHECK (projection_name = 'collaboration.stream'),
    generation UUID NOT NULL,
    thread_id UUID NOT NULL,
    event_cursor BIGINT NOT NULL CHECK (event_cursor > 0),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    event_kind JSONB NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, event_cursor),
    FOREIGN KEY (tenant_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (tenant_id, projection_name, generation)
        ON DELETE CASCADE
);

CREATE INDEX collaboration_thread_stream_projection
    ON erpx_collaboration_read.stream_projection
       (tenant_id, generation, thread_id, event_cursor);

CREATE TABLE erpx_collaboration_read.opaque_mention_inbox_projection (
    inbox_cursor BIGSERIAL NOT NULL,
    event_id UUID NOT NULL,
    tenant_id UUID NOT NULL,
    projection_name TEXT NOT NULL DEFAULT 'collaboration.inbox'
        CHECK (projection_name = 'collaboration.inbox'),
    generation UUID NOT NULL,
    recipient_principal_id UUID NOT NULL,
    thread_id UUID NOT NULL,
    mention_event_id UUID NOT NULL,
    anchor_record_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, inbox_cursor),
    UNIQUE (tenant_id, projection_name, generation, event_id),
    UNIQUE (
        tenant_id, projection_name, generation,
        recipient_principal_id, mention_event_id
    ),
    FOREIGN KEY (tenant_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (tenant_id, projection_name, generation)
        ON DELETE CASCADE
);

CREATE INDEX collaboration_inbox_projection_recipient
    ON erpx_collaboration_read.opaque_mention_inbox_projection
       (tenant_id, generation, recipient_principal_id, inbox_cursor);

-- The Item Directory is a disposable, tenant-local projection. Record owners
-- publish descriptors and explicit effective-principal grants; this schema is
-- never a business write authority and cannot infer access from descriptor
-- contents. Authorization rows are deliberately opaque and must be joined
-- before descriptor, relation, full-text match, or count data is selected.
CREATE TABLE erpx_collaboration_read.item_directory_projection_state (
    tenant_id UUID PRIMARY KEY,
    rebuild_generation BIGINT NOT NULL CHECK (rebuild_generation > 0),
    source_high_watermark BIGINT NOT NULL CHECK (source_high_watermark >= 0),
    applied_position BIGINT NOT NULL CHECK (applied_position >= 0),
    state TEXT NOT NULL CHECK (state IN ('ready', 'rebuilding', 'unavailable')),
    last_success_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (applied_position <= source_high_watermark),
    CHECK (state <> 'ready' OR applied_position = source_high_watermark)
);

CREATE TABLE erpx_collaboration_read.item_directory_descriptors (
    tenant_id UUID NOT NULL,
    record_id UUID NOT NULL,
    owner_context TEXT NOT NULL CHECK (octet_length(owner_context) BETWEEN 1 AND 64),
    record_type TEXT NOT NULL CHECK (octet_length(record_type) BETWEEN 1 AND 128),
    record_scope JSONB NOT NULL,
    authorization_policy_id UUID NOT NULL,
    authorization_policy_version BIGINT NOT NULL CHECK (authorization_policy_version > 0),
    descriptor JSONB,
    canonical_route TEXT,
    authorized_search_text TEXT NOT NULL DEFAULT '',
    search_vector TSVECTOR GENERATED ALWAYS AS (
        to_tsvector('simple', authorized_search_text)
    ) STORED,
    descriptor_version BIGINT NOT NULL CHECK (descriptor_version > 0),
    source_event_position BIGINT NOT NULL CHECK (source_event_position > 0),
    projected_at TIMESTAMPTZ NOT NULL,
    tombstoned_at TIMESTAMPTZ,
    PRIMARY KEY (tenant_id, record_id),
    CHECK (
        (tombstoned_at IS NULL AND descriptor IS NOT NULL AND canonical_route IS NOT NULL)
        OR (tombstoned_at IS NOT NULL AND descriptor IS NULL
            AND canonical_route IS NULL AND authorized_search_text = '')
    )
);

CREATE INDEX item_directory_authorized_search
    ON erpx_collaboration_read.item_directory_descriptors USING GIN (search_vector)
    WHERE tombstoned_at IS NULL;
CREATE INDEX item_directory_owner_position
    ON erpx_collaboration_read.item_directory_descriptors
       (tenant_id, owner_context, source_event_position, record_id);

CREATE TABLE erpx_collaboration_read.item_directory_effective_grants (
    tenant_id UUID NOT NULL,
    record_id UUID NOT NULL,
    principal_id UUID NOT NULL,
    authorization_policy_id UUID NOT NULL,
    policy_version BIGINT NOT NULL CHECK (policy_version > 0),
    can_view BOOLEAN NOT NULL,
    effective_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    source_event_position BIGINT NOT NULL CHECK (source_event_position > 0),
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, record_id, principal_id),
    CHECK (expires_at > effective_at),
    FOREIGN KEY (tenant_id, record_id)
        REFERENCES erpx_collaboration_read.item_directory_descriptors
            (tenant_id, record_id)
        ON DELETE CASCADE
);

CREATE INDEX item_directory_principal_grants
    ON erpx_collaboration_read.item_directory_effective_grants
       (tenant_id, principal_id, expires_at, record_id)
    WHERE can_view;

CREATE TABLE erpx_collaboration_read.item_directory_relations (
    tenant_id UUID NOT NULL,
    relation_id UUID NOT NULL,
    source_record_id UUID NOT NULL,
    target_record_id UUID NOT NULL,
    relation_type TEXT NOT NULL CHECK (
        relation_type IN (
            'relates-to', 'blocks', 'depends-on', 'implements',
            'supersedes', 'derived-from', 'evidence-for'
        )
    ),
    relation_version BIGINT NOT NULL CHECK (relation_version > 0),
    source_event_position BIGINT NOT NULL CHECK (source_event_position > 0),
    projected_at TIMESTAMPTZ NOT NULL,
    tombstoned_at TIMESTAMPTZ,
    PRIMARY KEY (tenant_id, relation_id),
    CHECK (source_record_id <> target_record_id),
    FOREIGN KEY (tenant_id, source_record_id)
        REFERENCES erpx_collaboration_read.item_directory_descriptors
            (tenant_id, record_id)
        ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, target_record_id)
        REFERENCES erpx_collaboration_read.item_directory_descriptors
            (tenant_id, record_id)
        ON DELETE CASCADE
);

-- Opaque high-water marks prevent delayed descriptor/grant/relation facts
-- from resurrecting revoked authority after purge. They contain no owner,
-- record type, title, route, lifecycle, or search payload.
CREATE TABLE erpx_collaboration_read.item_directory_tombstones (
    tenant_id UUID NOT NULL,
    record_id UUID NOT NULL,
    descriptor_version BIGINT NOT NULL CHECK (descriptor_version > 0),
    source_event_position BIGINT NOT NULL CHECK (source_event_position > 0),
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, record_id)
);

CREATE TABLE erpx_collaboration_read.item_directory_grant_tombstones (
    tenant_id UUID NOT NULL,
    record_id UUID NOT NULL,
    principal_id UUID NOT NULL,
    authorization_policy_id UUID NOT NULL,
    policy_version BIGINT NOT NULL CHECK (policy_version > 0),
    source_event_position BIGINT NOT NULL CHECK (source_event_position > 0),
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, record_id, principal_id)
);

CREATE TABLE erpx_collaboration_read.item_directory_relation_tombstones (
    tenant_id UUID NOT NULL,
    relation_id UUID NOT NULL,
    source_event_position BIGINT NOT NULL CHECK (source_event_position > 0),
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, relation_id)
);

CREATE INDEX item_directory_relation_source
    ON erpx_collaboration_read.item_directory_relations
       (tenant_id, source_record_id, relation_id)
    WHERE tombstoned_at IS NULL;
CREATE INDEX item_directory_relation_target
    ON erpx_collaboration_read.item_directory_relations
       (tenant_id, target_record_id, relation_id)
    WHERE tombstoned_at IS NULL;

DO $$
DECLARE
    table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'thread_projection', 'stream_projection',
        'opaque_mention_inbox_projection',
        'item_directory_projection_state', 'item_directory_descriptors',
        'item_directory_effective_grants', 'item_directory_relations',
        'item_directory_tombstones', 'item_directory_grant_tombstones',
        'item_directory_relation_tombstones'
    ]
    LOOP
        EXECUTE format(
            'ALTER TABLE erpx_collaboration_read.%I ENABLE ROW LEVEL SECURITY',
            table_name
        );
        EXECUTE format(
            'ALTER TABLE erpx_collaboration_read.%I FORCE ROW LEVEL SECURITY',
            table_name
        );
        EXECUTE format(
            'CREATE POLICY collaboration_read_tenant_isolation '
            'ON erpx_collaboration_read.%I '
            'USING (tenant_id::text = current_setting(''app.tenant_id'', true)) '
            'WITH CHECK (tenant_id::text = current_setting(''app.tenant_id'', true))',
            table_name
        );
    END LOOP;
END;
$$;
