-- Forward-only repair for the authority-lineage objects that were previously
-- introduced by editing the holding-read baseline. The baseline is immutable;
-- existing installations receive the same fail-closed behavior here.

CREATE TABLE IF NOT EXISTS erpx_authority_read.organization_nodes (
    holding_id UUID NOT NULL DEFAULT erpx_authority_read.current_holding_id(),
    node_id UUID NOT NULL,
    node_kind TEXT NOT NULL CHECK (node_kind IN (
        'holding','company','headquarters','branch','department','team'
    )),
    node_status TEXT NOT NULL CHECK (node_status IN ('active','suspended','archived')),
    parent_node_id UUID,
    lineage_node_ids UUID[] NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    source_event_position BIGINT NOT NULL CHECK (source_event_position > 0),
    changed_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,node_id),
    CHECK (cardinality(lineage_node_ids) BETWEEN 0 AND 4),
    CHECK (NOT node_id=ANY(lineage_node_ids)),
    CHECK (
        parent_node_id IS NOT DISTINCT FROM
            lineage_node_ids[cardinality(lineage_node_ids)]
    ),
    CHECK (
        (node_kind='holding' AND holding_id=node_id
            AND cardinality(lineage_node_ids)=0 AND parent_node_id IS NULL)
        OR (node_kind='company' AND cardinality(lineage_node_ids)=1
            AND lineage_node_ids[1]=holding_id)
        OR (node_kind IN ('headquarters','branch')
            AND cardinality(lineage_node_ids)=2
            AND lineage_node_ids[1]=holding_id)
        OR (node_kind='department' AND cardinality(lineage_node_ids) IN (2,3)
            AND lineage_node_ids[1]=holding_id)
        OR (node_kind='team' AND cardinality(lineage_node_ids) IN (3,4)
            AND lineage_node_ids[1]=holding_id)
    )
);

CREATE INDEX IF NOT EXISTS authority_read_organization_node_lineage
    ON erpx_authority_read.organization_nodes USING GIN (lineage_node_ids);
CREATE INDEX IF NOT EXISTS authority_read_organization_node_kind_status
    ON erpx_authority_read.organization_nodes
        (holding_id,node_kind,node_status,node_id);

ALTER TABLE erpx_authority_read.organization_nodes ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_authority_read.organization_nodes FORCE ROW LEVEL SECURITY;
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
          FROM pg_policies
         WHERE schemaname = 'erpx_authority_read'
           AND tablename = 'organization_nodes'
           AND policyname = 'authority_read_holding_isolation'
    ) THEN
        CREATE POLICY authority_read_holding_isolation
            ON erpx_authority_read.organization_nodes
            USING (holding_id = erpx_authority_read.current_holding_id())
            WITH CHECK (holding_id = erpx_authority_read.current_holding_id());
    END IF;
END;
$$;

CREATE OR REPLACE FUNCTION erpx_collaboration_read.audience_allows(
    audience JSONB,
    principal_id UUID,
    authority_scope JSONB,
    actor_realm TEXT,
    actor_portal TEXT
)
RETURNS BOOLEAN
LANGUAGE plpgsql
STABLE
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
