-- Disposable Phase 2 projections. Every relation query joins both endpoint
-- grants before exposing relation payload, count, or cursor. Attachment
-- download lookup similarly authorizes its record and thread in one snapshot.

-- Phase 1 originally strengthened this predicate in the baseline. Keep the
-- baseline immutable and apply the fail-closed hierarchy-aware replacement
-- forward instead.
CREATE OR REPLACE FUNCTION erpx_collaboration_read.scope_contains(authority JSONB, target JSONB)
RETURNS BOOLEAN
LANGUAGE plpgsql
STABLE
AS $$
DECLARE
    uuid_pattern CONSTANT TEXT :=
        '^[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}$';
    nil_uuid CONSTANT UUID := '00000000-0000-0000-0000-000000000000';
    authority_kind TEXT;
    authority_body JSONB;
    authority_holding_text TEXT;
    authority_company_text TEXT;
    authority_branch_text TEXT;
    authority_node_text TEXT;
    authority_holding UUID;
    authority_company UUID;
    authority_branch UUID;
    authority_node UUID;
    target_kind TEXT;
    target_body JSONB;
    target_holding_text TEXT;
    target_company_text TEXT;
    target_branch_text TEXT;
    target_node_text TEXT;
    target_holding UUID;
    target_company UUID;
    target_branch UUID;
    target_node UUID;
    target_node_kind TEXT;
    target_lineage UUID[];
BEGIN
    -- AuthorityScope is an externally tagged enum. Reject unknown, partial,
    -- over-posted, non-canonical, nil, and non-string UUID fields before any
    -- hierarchy lookup so malformed data can never broaden access.
    IF authority IS NULL OR target IS NULL THEN
        RETURN FALSE;
    END IF;
    IF jsonb_typeof(authority) = 'string' THEN
        IF authority <> to_jsonb('Platform'::TEXT) THEN
            RETURN FALSE;
        END IF;
        authority_kind := 'Platform';
    ELSIF jsonb_typeof(authority) = 'object' THEN
        IF (SELECT count(*) FROM jsonb_object_keys(authority)) <> 1 THEN
            RETURN FALSE;
        END IF;
        SELECT entry.key, entry.value
          INTO authority_kind, authority_body
          FROM jsonb_each(authority) AS entry(key,value);
        CASE authority_kind
            WHEN 'Holding' THEN
                authority_holding_text := authority_body ->> 'holding_id';
                IF authority <> jsonb_build_object(
                    'Holding', jsonb_build_object('holding_id', authority_holding_text)
                ) THEN
                    RETURN FALSE;
                END IF;
            WHEN 'Company' THEN
                authority_holding_text := authority_body ->> 'holding_id';
                authority_company_text := authority_body ->> 'company_id';
                IF authority <> jsonb_build_object(
                    'Company', jsonb_build_object(
                        'holding_id', authority_holding_text,
                        'company_id', authority_company_text
                    )
                ) THEN
                    RETURN FALSE;
                END IF;
            WHEN 'Branch' THEN
                authority_holding_text := authority_body ->> 'holding_id';
                authority_company_text := authority_body ->> 'company_id';
                authority_branch_text := authority_body ->> 'branch_id';
                IF authority <> jsonb_build_object(
                    'Branch', jsonb_build_object(
                        'holding_id', authority_holding_text,
                        'company_id', authority_company_text,
                        'branch_id', authority_branch_text
                    )
                ) THEN
                    RETURN FALSE;
                END IF;
            WHEN 'Subtree' THEN
                authority_holding_text := authority_body ->> 'holding_id';
                authority_node_text := authority_body ->> 'organization_node_id';
                IF authority <> jsonb_build_object(
                    'Subtree', jsonb_build_object(
                        'holding_id', authority_holding_text,
                        'organization_node_id', authority_node_text
                    )
                ) THEN
                    RETURN FALSE;
                END IF;
            ELSE
                RETURN FALSE;
        END CASE;
    ELSE
        RETURN FALSE;
    END IF;

    IF jsonb_typeof(target) = 'string' THEN
        IF target <> to_jsonb('Platform'::TEXT) THEN
            RETURN FALSE;
        END IF;
        target_kind := 'Platform';
    ELSIF jsonb_typeof(target) = 'object' THEN
        IF (SELECT count(*) FROM jsonb_object_keys(target)) <> 1 THEN
            RETURN FALSE;
        END IF;
        SELECT entry.key, entry.value
          INTO target_kind, target_body
          FROM jsonb_each(target) AS entry(key,value);
        CASE target_kind
            WHEN 'Holding' THEN
                target_holding_text := target_body ->> 'holding_id';
                IF target <> jsonb_build_object(
                    'Holding', jsonb_build_object('holding_id', target_holding_text)
                ) THEN
                    RETURN FALSE;
                END IF;
            WHEN 'Company' THEN
                target_holding_text := target_body ->> 'holding_id';
                target_company_text := target_body ->> 'company_id';
                IF target <> jsonb_build_object(
                    'Company', jsonb_build_object(
                        'holding_id', target_holding_text,
                        'company_id', target_company_text
                    )
                ) THEN
                    RETURN FALSE;
                END IF;
            WHEN 'Branch' THEN
                target_holding_text := target_body ->> 'holding_id';
                target_company_text := target_body ->> 'company_id';
                target_branch_text := target_body ->> 'branch_id';
                IF target <> jsonb_build_object(
                    'Branch', jsonb_build_object(
                        'holding_id', target_holding_text,
                        'company_id', target_company_text,
                        'branch_id', target_branch_text
                    )
                ) THEN
                    RETURN FALSE;
                END IF;
            WHEN 'Subtree' THEN
                target_holding_text := target_body ->> 'holding_id';
                target_node_text := target_body ->> 'organization_node_id';
                IF target <> jsonb_build_object(
                    'Subtree', jsonb_build_object(
                        'holding_id', target_holding_text,
                        'organization_node_id', target_node_text
                    )
                ) THEN
                    RETURN FALSE;
                END IF;
            ELSE
                RETURN FALSE;
        END CASE;
    ELSE
        RETURN FALSE;
    END IF;

    IF authority_kind <> 'Platform' THEN
        IF authority_holding_text IS NULL
           OR authority_holding_text !~* uuid_pattern THEN
            RETURN FALSE;
        END IF;
        authority_holding := authority_holding_text::UUID;
        IF authority_holding = nil_uuid THEN
            RETURN FALSE;
        END IF;
    END IF;
    IF authority_kind IN ('Company','Branch') THEN
        IF authority_company_text IS NULL
           OR authority_company_text !~* uuid_pattern THEN
            RETURN FALSE;
        END IF;
        authority_company := authority_company_text::UUID;
        IF authority_company = nil_uuid THEN
            RETURN FALSE;
        END IF;
    END IF;
    IF authority_kind = 'Branch' THEN
        IF authority_branch_text IS NULL
           OR authority_branch_text !~* uuid_pattern THEN
            RETURN FALSE;
        END IF;
        authority_branch := authority_branch_text::UUID;
        IF authority_branch = nil_uuid THEN
            RETURN FALSE;
        END IF;
    END IF;
    IF authority_kind = 'Subtree' THEN
        IF authority_node_text IS NULL
           OR authority_node_text !~* uuid_pattern THEN
            RETURN FALSE;
        END IF;
        authority_node := authority_node_text::UUID;
        IF authority_node = nil_uuid THEN
            RETURN FALSE;
        END IF;
    END IF;

    IF target_kind <> 'Platform' THEN
        IF target_holding_text IS NULL OR target_holding_text !~* uuid_pattern THEN
            RETURN FALSE;
        END IF;
        target_holding := target_holding_text::UUID;
        IF target_holding = nil_uuid THEN
            RETURN FALSE;
        END IF;
    END IF;
    IF target_kind IN ('Company','Branch') THEN
        IF target_company_text IS NULL OR target_company_text !~* uuid_pattern THEN
            RETURN FALSE;
        END IF;
        target_company := target_company_text::UUID;
        IF target_company = nil_uuid THEN
            RETURN FALSE;
        END IF;
    END IF;
    IF target_kind = 'Branch' THEN
        IF target_branch_text IS NULL OR target_branch_text !~* uuid_pattern THEN
            RETURN FALSE;
        END IF;
        target_branch := target_branch_text::UUID;
        IF target_branch = nil_uuid THEN
            RETURN FALSE;
        END IF;
    END IF;
    IF target_kind = 'Subtree' THEN
        IF target_node_text IS NULL OR target_node_text !~* uuid_pattern THEN
            RETURN FALSE;
        END IF;
        target_node := target_node_text::UUID;
        IF target_node = nil_uuid THEN
            RETURN FALSE;
        END IF;
    END IF;

    IF authority_kind <> 'Platform'
       AND (target_kind = 'Platform' OR authority_holding <> target_holding) THEN
        RETURN FALSE;
    END IF;

    -- An exact Subtree is canonical only for Department or Team. Validate the
    -- entire active path in the Organization-authored cache, not just the
    -- target row. A missing, inactive, wrong-kind, or contradictory ancestor
    -- therefore denies even if a UUID happens to match a structured scope.
    IF target_kind = 'Subtree' THEN
        SELECT candidate.node_kind, candidate.lineage_node_ids
          INTO target_node_kind, target_lineage
          FROM erpx_authority_read.organization_nodes AS candidate
         WHERE candidate.holding_id = target_holding
           AND candidate.node_id = target_node
           AND candidate.node_status = 'active'
           AND candidate.node_kind IN ('department','team')
           AND candidate.lineage_node_ids[1] = target_holding
           AND cardinality(candidate.lineage_node_ids) = cardinality(
               ARRAY(
                   SELECT DISTINCT lineage_node_id
                     FROM unnest(candidate.lineage_node_ids) AS lineage(lineage_node_id)
               )
           )
           AND NOT (candidate.node_id = ANY(candidate.lineage_node_ids))
           AND candidate.parent_node_id IS NOT DISTINCT FROM
               candidate.lineage_node_ids[cardinality(candidate.lineage_node_ids)]
           AND (
               (candidate.node_kind = 'department'
                    AND cardinality(candidate.lineage_node_ids) IN (2,3))
               OR (candidate.node_kind = 'team'
                    AND cardinality(candidate.lineage_node_ids) IN (3,4))
           )
           AND NOT EXISTS (
               SELECT 1
                 FROM unnest(candidate.lineage_node_ids) WITH ORDINALITY
                      AS path(node_id,depth)
                 LEFT JOIN erpx_authority_read.organization_nodes AS ancestor
                   ON ancestor.holding_id = candidate.holding_id
                  AND ancestor.node_id = path.node_id
                WHERE ancestor.node_id IS NULL
                   OR ancestor.node_status <> 'active'
                   OR ancestor.node_kind IS DISTINCT FROM CASE
                       WHEN path.depth = 1 THEN 'holding'
                       WHEN path.depth = 2 THEN 'company'
                       WHEN candidate.node_kind = 'department'
                            AND cardinality(candidate.lineage_node_ids) = 3
                            AND path.depth = 3 THEN 'branch'
                       WHEN candidate.node_kind = 'team'
                            AND cardinality(candidate.lineage_node_ids) = 3
                            AND path.depth = 3 THEN 'department'
                       WHEN candidate.node_kind = 'team'
                            AND cardinality(candidate.lineage_node_ids) = 4
                            AND path.depth = 3 THEN 'branch'
                       WHEN candidate.node_kind = 'team'
                            AND cardinality(candidate.lineage_node_ids) = 4
                            AND path.depth = 4 THEN 'department'
                       ELSE NULL
                   END
                   OR ancestor.lineage_node_ids IS DISTINCT FROM CASE
                       WHEN path.depth = 1 THEN ARRAY[]::UUID[]
                       ELSE candidate.lineage_node_ids[1:(path.depth::INTEGER - 1)]
                   END
                   OR ancestor.parent_node_id IS DISTINCT FROM CASE
                       WHEN path.depth = 1 THEN NULL::UUID
                       ELSE candidate.lineage_node_ids[path.depth::INTEGER - 1]
                   END
           );
        IF NOT FOUND THEN
            RETURN FALSE;
        END IF;

        IF authority_kind = 'Platform' OR authority_kind = 'Holding' THEN
            RETURN TRUE;
        END IF;
        IF authority_kind = 'Company' THEN
            RETURN target_lineage[2] = authority_company;
        END IF;
        IF authority_kind = 'Branch' THEN
            RETURN (
                    (target_node_kind = 'department'
                        AND cardinality(target_lineage) = 3)
                    OR (target_node_kind = 'team'
                        AND cardinality(target_lineage) = 4)
                )
                AND target_lineage[2] = authority_company
                AND target_lineage[3] = authority_branch;
        END IF;
        IF authority_kind = 'Subtree' THEN
            IF authority_node = target_node THEN
                RETURN TRUE;
            END IF;
            -- The already validated target chain proves the prefix of this
            -- ancestor. Requiring a Department/Team row rejects Subtree scopes
            -- that merely encode a Company or Branch UUID.
            RETURN EXISTS (
                SELECT 1
                  FROM erpx_authority_read.organization_nodes AS ancestor
                 WHERE ancestor.holding_id = target_holding
                   AND ancestor.node_id = authority_node
                   AND ancestor.node_id = ANY(target_lineage)
                   AND ancestor.node_kind IN ('department','team')
                   AND ancestor.node_status = 'active'
            );
        END IF;
        RETURN FALSE;
    END IF;

    IF authority_kind = 'Platform' THEN
        RETURN TRUE;
    END IF;
    IF authority_kind = 'Holding' THEN
        RETURN target_kind IN ('Holding','Company','Branch');
    END IF;
    IF authority_kind = 'Company' THEN
        RETURN target_kind IN ('Company', 'Branch')
            AND authority_company = target_company;
    END IF;
    IF authority_kind = 'Branch' THEN
        RETURN target_kind = 'Branch'
            AND authority_company = target_company
            AND authority_branch = target_branch;
    END IF;
    -- Exact Department/Team authority never grants access to an ancestor.
    RETURN FALSE;
END;
$$;


ALTER TABLE erpx_collaboration_read.item_directory_descriptors
    ADD COLUMN display_label TEXT
        CHECK (display_label IS NULL OR octet_length(display_label) BETWEEN 1 AND 255);

CREATE INDEX item_directory_authorized_picker
    ON erpx_collaboration_read.item_directory_descriptors
       (holding_id, source_event_position DESC, record_id)
    WHERE tombstoned_at IS NULL AND display_label IS NOT NULL;

CREATE TABLE erpx_collaboration_read.record_relation_projection (
    holding_id UUID NOT NULL,
    relation_id UUID NOT NULL,
    source_record_id UUID NOT NULL,
    target_record_id UUID NOT NULL,
    source_record_ref JSONB NOT NULL,
    target_record_ref JSONB NOT NULL,
    graph_thread_id UUID NOT NULL,
    relation_kind TEXT NOT NULL CHECK (relation_kind IN (
        'relates_to','blocks','depends_on','implements',
        'supersedes','derived_from','evidence_for'
    )),
    relation_version BIGINT NOT NULL CHECK (relation_version > 0),
    graph_version BIGINT NOT NULL CHECK (graph_version > 0),
    relation_document JSONB NOT NULL,
    created_by_principal_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    revoked_by_principal_id UUID,
    revoked_at TIMESTAMPTZ,
    committed_sequence BIGINT NOT NULL CHECK (committed_sequence > 0),
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, relation_id),
    UNIQUE (holding_id, source_record_id, graph_version),
    CHECK (source_record_id <> target_record_id),
    CHECK (source_record_ref ->> 'id' = source_record_id::TEXT),
    CHECK (target_record_ref ->> 'id' = target_record_id::TEXT),
    CHECK ((revoked_by_principal_id IS NULL) = (revoked_at IS NULL)),
    CHECK (revoked_at IS NULL OR revoked_at >= created_at)
);

CREATE INDEX collaboration_relation_projection_source
    ON erpx_collaboration_read.record_relation_projection
       (holding_id, source_record_id, committed_sequence, relation_id);
CREATE INDEX collaboration_relation_projection_target
    ON erpx_collaboration_read.record_relation_projection
       (holding_id, target_record_id, committed_sequence, relation_id);

CREATE TABLE erpx_collaboration_read.attachment_projection (
    holding_id UUID NOT NULL,
    attachment_id UUID NOT NULL,
    object_id UUID NOT NULL,
    object_key TEXT NOT NULL,
    anchor_record_id UUID NOT NULL,
    anchor_record_ref JSONB NOT NULL,
    thread_id UUID,
    uploader_principal_id UUID NOT NULL,
    classification TEXT NOT NULL CHECK (
        classification IN ('public','internal','confidential','restricted')
    ),
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
    PRIMARY KEY (holding_id, attachment_id),
    UNIQUE (holding_id, object_id),
    CHECK (anchor_record_ref ->> 'id' = anchor_record_id::TEXT),
    CHECK ((lifecycle='ready') = (ready_at IS NOT NULL)),
    CHECK (lifecycle NOT IN ('ready','quarantined') OR thread_id IS NOT NULL)
);

CREATE INDEX collaboration_ready_attachment_anchor
    ON erpx_collaboration_read.attachment_projection
       (holding_id, anchor_record_id, thread_id, attachment_id)
    WHERE lifecycle='ready';

DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'record_relation_projection','attachment_projection'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_collaboration_read.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_collaboration_read.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY collaboration_read_holding_isolation '
            'ON erpx_collaboration_read.%I '
            'USING (holding_id::text = current_setting(''app.holding_id'', true)) '
            'WITH CHECK (holding_id::text = current_setting(''app.holding_id'', true))',
            table_name
        );
    END LOOP;
END;
$$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM erpx_meta.schema_manifest
        WHERE singleton=TRUE AND schema_revision=2 AND manifest_version=1
          AND database_role='holding_read' AND migration_set='holding-read'
    ) THEN
        RAISE EXCEPTION 'holding-read current manifest is not current';
    END IF;
END;
$$;
