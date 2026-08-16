-- Generation-managed detail projections for Manufacturing specifications and
-- governed Planning records. These tables are disposable read models only.
CREATE TABLE erpx_manufacturing_read.specification_projection (
    tenant_id uuid NOT NULL DEFAULT erpx_manufacturing_read.current_tenant_id(),
    projection_name text NOT NULL DEFAULT 'manufacturing.orders'
        CHECK (projection_name = 'manufacturing.orders'),
    generation uuid NOT NULL,
    partition_key text NOT NULL,
    specification_id uuid NOT NULL,
    authority_scope jsonb NOT NULL,
    lifecycle_state text NOT NULL,
    projection jsonb NOT NULL,
    source_aggregate_version bigint NOT NULL CHECK (source_aggregate_version > 0),
    source_position bigint NOT NULL CHECK (source_position > 0),
    projected_at timestamptz NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, specification_id),
    UNIQUE (tenant_id, projection_name, generation, specification_id),
    FOREIGN KEY (tenant_id, projection_name, generation)
      REFERENCES erpx_cqrs_read.projection_generations(tenant_id, projection_name, generation)
      ON DELETE CASCADE
);

CREATE TABLE erpx_planning_read.governed_record_projection (
    tenant_id uuid NOT NULL DEFAULT erpx_planning_read.current_tenant_id(),
    projection_name text NOT NULL DEFAULT 'planning.runs'
        CHECK (projection_name = 'planning.runs'),
    generation uuid NOT NULL,
    partition_key text NOT NULL,
    record_id uuid NOT NULL,
    record_kind text NOT NULL CHECK (record_kind IN ('scenario', 'recommendation')),
    authority_scope jsonb NOT NULL,
    lifecycle_state text NOT NULL,
    projection jsonb NOT NULL,
    source_aggregate_version bigint NOT NULL CHECK (source_aggregate_version > 0),
    source_position bigint NOT NULL CHECK (source_position > 0),
    projected_at timestamptz NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, record_id),
    UNIQUE (tenant_id, projection_name, generation, record_id),
    FOREIGN KEY (tenant_id, projection_name, generation)
      REFERENCES erpx_cqrs_read.projection_generations(tenant_id, projection_name, generation)
      ON DELETE CASCADE
);

ALTER TABLE erpx_manufacturing_read.specification_projection ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_manufacturing_read.specification_projection FORCE ROW LEVEL SECURITY;
CREATE POLICY manufacturing_specification_tenant_isolation
    ON erpx_manufacturing_read.specification_projection
    USING (tenant_id = erpx_manufacturing_read.current_tenant_id())
    WITH CHECK (tenant_id = erpx_manufacturing_read.current_tenant_id());

ALTER TABLE erpx_planning_read.governed_record_projection ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_planning_read.governed_record_projection FORCE ROW LEVEL SECURITY;
CREATE POLICY planning_governed_record_tenant_isolation
    ON erpx_planning_read.governed_record_projection
    USING (tenant_id = erpx_planning_read.current_tenant_id())
    WITH CHECK (tenant_id = erpx_planning_read.current_tenant_id());

UPDATE erpx_meta.schema_manifest
SET installed_at = now()
WHERE singleton AND schema_generation = 2
  AND database_role = 'tenant_read' AND migration_set = 'tenant-read-v2';
