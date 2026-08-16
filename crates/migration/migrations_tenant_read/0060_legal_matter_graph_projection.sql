-- Disposable Legal relationship graph projection rebuilt from matter snapshots.

CREATE TABLE erpx_legal_read.matter_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'legal.matters'
        CHECK (projection_name = 'legal.matters'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    matter_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    reference TEXT NOT NULL,
    title TEXT NOT NULL,
    matter_status TEXT NOT NULL CHECK (matter_status IN ('open', 'closed')),
    links JSONB NOT NULL CHECK (jsonb_typeof(links) = 'array'),
    legal_hold JSONB NOT NULL,
    created_by TEXT NOT NULL,
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, matter_id),
    FOREIGN KEY (tenant_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (tenant_id, projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (tenant_id, generation, scope_key, reference)
);

CREATE INDEX legal_read_matter_scope_status
    ON erpx_legal_read.matter_projection
        (tenant_id, generation, scope_key, matter_status, aggregate_updated_at DESC, matter_id DESC);
CREATE INDEX legal_read_matter_links_gin
    ON erpx_legal_read.matter_projection USING GIN (links jsonb_path_ops);

DO $$
DECLARE schema_name TEXT; table_name TEXT;
BEGIN
    FOR schema_name, table_name IN SELECT * FROM (VALUES
        ('erpx_legal_read','matter_projection')
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
