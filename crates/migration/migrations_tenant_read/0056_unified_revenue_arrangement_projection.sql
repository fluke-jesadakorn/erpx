-- Disposable Sales revenue-arrangement workbench projection. Source state is
-- rebuilt exclusively from Sales snapshot outbox events.

CREATE TABLE erpx_sales_read.revenue_arrangement_projection (
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
    PRIMARY KEY (projection_name, generation, partition_key, arrangement_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (generation, sales_order_id)
);

CREATE INDEX sales_read_revenue_arrangement_scope_customer_status
    ON erpx_sales_read.revenue_arrangement_projection
        (generation, scope_key, customer_id, arrangement_status,
         aggregate_updated_at DESC, arrangement_id DESC);
