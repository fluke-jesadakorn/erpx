-- Disposable Projects projections rebuilt from typed Projects projection facts.
CREATE SCHEMA IF NOT EXISTS erpx_projects_read;

CREATE OR REPLACE FUNCTION erpx_projects_read.current_tenant_id()
RETURNS UUID LANGUAGE SQL STABLE AS $$
    SELECT nullif(current_setting('app.tenant_id', true), '')::uuid
$$;

CREATE TABLE erpx_projects_read.project_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_projects_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'projects.projects' CHECK (projection_name = 'projects.projects'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    project_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    project_code TEXT NOT NULL,
    project_class TEXT NOT NULL,
    project_status TEXT NOT NULL,
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, project_id),
    FOREIGN KEY (tenant_id, projection_name, generation) REFERENCES erpx_cqrs_read.projection_generations(tenant_id, projection_name, generation) ON DELETE CASCADE,
    UNIQUE (tenant_id, generation, scope_key, project_code)
);

CREATE TABLE erpx_projects_read.hierarchy_closure_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_projects_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'projects.hierarchy' CHECK (projection_name = 'projects.hierarchy'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    project_id UUID NOT NULL,
    ancestor_node_id UUID NOT NULL,
    descendant_node_id UUID NOT NULL,
    depth SMALLINT NOT NULL CHECK (depth BETWEEN 0 AND 2),
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, project_id, ancestor_node_id, descendant_node_id),
    FOREIGN KEY (tenant_id, projection_name, generation) REFERENCES erpx_cqrs_read.projection_generations(tenant_id, projection_name, generation) ON DELETE CASCADE
);

CREATE TABLE erpx_projects_read.dependency_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_projects_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'projects.dependencies' CHECK (projection_name = 'projects.dependencies'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    dependency_id UUID NOT NULL,
    source_project_id UUID NOT NULL,
    target_project_id UUID NOT NULL,
    dependency_status TEXT NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    state JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, dependency_id),
    FOREIGN KEY (tenant_id, projection_name, generation) REFERENCES erpx_cqrs_read.projection_generations(tenant_id, projection_name, generation) ON DELETE CASCADE
);

CREATE TABLE erpx_projects_read.handoff_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_projects_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'projects.handoffs' CHECK (projection_name = 'projects.handoffs'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    handoff_id UUID NOT NULL,
    project_id UUID NOT NULL,
    handoff_status TEXT NOT NULL,
    owner_context TEXT NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    state JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, handoff_id),
    FOREIGN KEY (tenant_id, projection_name, generation) REFERENCES erpx_cqrs_read.projection_generations(tenant_id, projection_name, generation) ON DELETE CASCADE
);

CREATE INDEX projects_read_scope_status ON erpx_projects_read.project_projection(tenant_id, generation, scope_key, project_status, aggregate_updated_at DESC, project_id DESC);
CREATE INDEX projects_read_closure_descendant ON erpx_projects_read.hierarchy_closure_projection(tenant_id, generation, project_id, descendant_node_id, depth);
CREATE INDEX projects_read_dependency_active ON erpx_projects_read.dependency_projection(tenant_id, generation, source_project_id, target_project_id) WHERE dependency_status = 'accepted';
CREATE INDEX projects_read_handoff_pending ON erpx_projects_read.handoff_projection(tenant_id, generation, project_id, handoff_status) WHERE handoff_status IN ('ready','uncertain');

DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'project_projection','hierarchy_closure_projection',
        'dependency_projection','handoff_projection'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_projects_read.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_projects_read.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY projects_read_tenant_isolation ON erpx_projects_read.%I '
            'USING (tenant_id = erpx_projects_read.current_tenant_id()) '
            'WITH CHECK (tenant_id = erpx_projects_read.current_tenant_id())',
            table_name
        );
    END LOOP;
END;
$$;
