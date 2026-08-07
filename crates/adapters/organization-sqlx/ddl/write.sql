CREATE SCHEMA IF NOT EXISTS erpx_organization_write;

CREATE TABLE erpx_organization_write.hierarchy_guards (
    node_id UUID PRIMARY KEY,
    node_kind TEXT NOT NULL CHECK (node_kind IN (
        'holding', 'company', 'headquarters', 'branch', 'department', 'team'
    )),
    parent_node_id UUID,
    lineage_node_ids UUID[] NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID,
    branch_id UUID,
    department_id UUID,
    node_code TEXT NOT NULL,
    node_status TEXT NOT NULL CHECK (node_status IN ('active', 'suspended', 'archived')),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (NOT node_id = ANY(lineage_node_ids))
);

CREATE UNIQUE INDEX organization_sibling_code
    ON erpx_organization_write.hierarchy_guards
        (holding_id, COALESCE(parent_node_id, holding_id), node_code)
    WHERE node_status <> 'archived';
