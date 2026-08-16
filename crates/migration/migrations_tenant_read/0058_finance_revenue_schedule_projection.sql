-- Disposable billing-independent revenue-recognition workbench.

CREATE TABLE erpx_finance_read.revenue_schedule_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'finance.revenue_schedules'
        CHECK (projection_name = 'finance.revenue_schedules'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    schedule_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    arrangement_id UUID NOT NULL,
    sales_order_id UUID NOT NULL,
    customer_id UUID NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    allocated_minor BIGINT NOT NULL CHECK (allocated_minor > 0),
    recognized_minor BIGINT NOT NULL CHECK (
        recognized_minor >= 0 AND recognized_minor <= allocated_minor
    ),
    schedule_status TEXT NOT NULL CHECK (schedule_status IN ('active', 'completed')),
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, schedule_id),
    FOREIGN KEY (tenant_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (tenant_id, projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (tenant_id, generation, arrangement_id)
);

CREATE INDEX finance_read_revenue_schedule_scope_customer_status
    ON erpx_finance_read.revenue_schedule_projection
        (tenant_id, generation, scope_key, customer_id, schedule_status,
         aggregate_updated_at DESC, schedule_id DESC);

DO $$
DECLARE schema_name TEXT; table_name TEXT;
BEGIN
    FOR schema_name, table_name IN SELECT * FROM (VALUES
        ('erpx_finance_read','revenue_schedule_projection')
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
