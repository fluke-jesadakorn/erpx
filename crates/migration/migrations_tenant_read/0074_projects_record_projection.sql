-- Active-generation detail rows for workflow templates and typed handoffs.
CREATE TABLE erpx_projects_read.project_record_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_projects_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'projects.records' CHECK (projection_name = 'projects.records'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    record_id UUID NOT NULL,
    record_kind TEXT NOT NULL CHECK (record_kind IN ('workflow_template', 'handoff')),
    project_id UUID,
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    lifecycle_status TEXT NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    state JSONB NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, record_id),
    FOREIGN KEY (tenant_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations(tenant_id, projection_name, generation)
        ON DELETE CASCADE,
    CHECK ((record_kind = 'workflow_template' AND project_id IS NULL)
        OR (record_kind = 'handoff' AND project_id IS NOT NULL))
);

CREATE INDEX projects_record_read_scope
    ON erpx_projects_read.project_record_projection
    (tenant_id, generation, scope_key, record_kind, aggregate_updated_at DESC, record_id DESC);

ALTER TABLE erpx_projects_read.project_record_projection ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_projects_read.project_record_projection FORCE ROW LEVEL SECURITY;
CREATE POLICY projects_record_read_tenant_isolation
    ON erpx_projects_read.project_record_projection
    USING (tenant_id = erpx_projects_read.current_tenant_id())
    WITH CHECK (tenant_id = erpx_projects_read.current_tenant_id());

UPDATE erpx_meta.schema_manifest
SET installed_at = now()
WHERE singleton AND schema_generation = 2
  AND database_role = 'tenant_read' AND migration_set = 'tenant-read-v2';
