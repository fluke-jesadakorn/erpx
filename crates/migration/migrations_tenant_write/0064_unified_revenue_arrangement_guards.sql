-- Sales-owned mixed-revenue arrangements. Migration 0081 replaces the JSON
-- compatibility representation as authority with typed dimension, obligation,
-- and fulfillment child relations.

CREATE TABLE erpx_sales_write.revenue_arrangement_guards (
    arrangement_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    sales_order_id UUID NOT NULL UNIQUE,
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
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (updated_at >= created_at),
    FOREIGN KEY (sales_order_id)
        REFERENCES erpx_sales_write.sales_order_guards (sales_order_id)
);

CREATE INDEX sales_revenue_arrangement_scope_customer_status
    ON erpx_sales_write.revenue_arrangement_guards
        (scope_key, customer_id, arrangement_status, updated_at DESC, arrangement_id DESC);
