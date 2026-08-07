CREATE SCHEMA IF NOT EXISTS erpx_organization_read;

CREATE TABLE erpx_organization_read.node_projection (
    projection_name TEXT NOT NULL DEFAULT 'organization.nodes',
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    node_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    node_kind TEXT NOT NULL,
    node_status TEXT NOT NULL,
    parent_node_id UUID,
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    placement JSONB NOT NULL,
    node_code TEXT NOT NULL,
    display_name TEXT NOT NULL,
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, node_id)
);
