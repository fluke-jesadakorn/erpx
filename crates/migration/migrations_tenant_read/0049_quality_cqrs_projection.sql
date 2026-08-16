CREATE SCHEMA IF NOT EXISTS erpx_quality_read;

CREATE TABLE erpx_quality_read.inspection_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'quality.inspections'
        CHECK (projection_name = 'quality.inspections'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    inspection_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    reference TEXT NOT NULL,
    tenant_scope JSONB NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    inspection_kind TEXT NOT NULL
        CHECK (inspection_kind IN ('incoming', 'in_process', 'final')),
    product_id UUID NOT NULL,
    lot_reference TEXT,
    quantity NUMERIC(20, 0) NOT NULL CHECK (quantity > 0),
    passed_quantity NUMERIC(20, 0) NOT NULL CHECK (passed_quantity >= 0),
    failed_quantity NUMERIC(20, 0) NOT NULL CHECK (failed_quantity >= 0),
    inspection_status TEXT NOT NULL
        CHECK (inspection_status IN (
            'draft', 'in_progress', 'passed', 'failed', 'released', 'cancelled'
        )),
    disposition_note TEXT,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    state JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, inspection_id),
    FOREIGN KEY (tenant_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (tenant_id, projection_name, generation)
        ON DELETE CASCADE,
    CHECK (passed_quantity + failed_quantity <= quantity)
);

CREATE UNIQUE INDEX quality_projection_reference
    ON erpx_quality_read.inspection_projection
        (tenant_id, generation, holding_id, company_id,
         COALESCE(branch_id, '00000000-0000-0000-0000-000000000000'::uuid), reference);

CREATE INDEX quality_projection_scope_updated
    ON erpx_quality_read.inspection_projection
        (tenant_id, generation, holding_id, company_id, branch_id,
         aggregate_updated_at DESC, inspection_id DESC);

CREATE INDEX quality_projection_status_updated
    ON erpx_quality_read.inspection_projection
        (tenant_id, generation, inspection_status, aggregate_updated_at DESC, inspection_id DESC);

CREATE INDEX quality_projection_product_updated
    ON erpx_quality_read.inspection_projection
        (tenant_id, generation, product_id, aggregate_updated_at DESC, inspection_id DESC);

DO $$
DECLARE schema_name TEXT; table_name TEXT;
BEGIN
    FOR schema_name, table_name IN SELECT * FROM (VALUES
        ('erpx_quality_read','inspection_projection')
    ) AS protected(schema_name, table_name)
    LOOP
        EXECUTE format('ALTER TABLE %I.%I ENABLE ROW LEVEL SECURITY', schema_name, table_name);
        EXECUTE format('ALTER TABLE %I.%I FORCE ROW LEVEL SECURITY', schema_name, table_name);
        EXECUTE format(
            'CREATE POLICY tenant_isolation ON %I.%I '
            'USING (tenant_id = erpx_cqrs_read.current_tenant_id()) '
            'WITH CHECK (tenant_id = erpx_cqrs_read.current_tenant_id())',
            schema_name, table_name
        );
    END LOOP;
END;
$$;
