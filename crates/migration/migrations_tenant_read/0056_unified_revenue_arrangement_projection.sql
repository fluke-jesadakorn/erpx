-- Disposable Sales revenue-arrangement workbench projection. Source state is
-- rebuilt exclusively from Sales snapshot outbox events.

CREATE TABLE erpx_sales_read.revenue_arrangement_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'sales.revenue_arrangements'
        CHECK (projection_name = 'sales.revenue_arrangements'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    arrangement_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    sales_order_id UUID NOT NULL,
    customer_id UUID NOT NULL,
    legal_contract_id UUID,
    order_code TEXT NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    arrangement_status TEXT NOT NULL CHECK (arrangement_status IN (
        'active', 'completed', 'cancelled'
    )),
    dimensions JSONB NOT NULL,
    obligations JSONB NOT NULL CHECK (jsonb_typeof(obligations) = 'array'),
    fulfillments JSONB NOT NULL CHECK (jsonb_typeof(fulfillments) = 'array'),
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, arrangement_id),
    FOREIGN KEY (tenant_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (tenant_id, projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (tenant_id, generation, sales_order_id)
);

CREATE INDEX sales_read_revenue_arrangement_scope_customer_status
    ON erpx_sales_read.revenue_arrangement_projection
        (tenant_id, generation, scope_key, customer_id, arrangement_status,
         aggregate_updated_at DESC, arrangement_id DESC);

DO $$
DECLARE schema_name TEXT; table_name TEXT;
BEGIN
    FOR schema_name, table_name IN SELECT * FROM (VALUES
        ('erpx_sales_read','revenue_arrangement_projection')
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
