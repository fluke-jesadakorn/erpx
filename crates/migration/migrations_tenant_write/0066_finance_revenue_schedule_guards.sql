-- Finance-owned IFRS/TFRS 15 schedule created from a versioned Sales revenue
-- arrangement. Migration 0081 adds typed arrangement, obligation, dimension,
-- and recognition authority children.

CREATE TABLE erpx_finance_write.revenue_schedule_guards (
    schedule_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    arrangement_id UUID NOT NULL UNIQUE,
    sales_order_id UUID NOT NULL,
    customer_id UUID NOT NULL,
    source_context TEXT NOT NULL CHECK (source_context = 'sales'),
    source_aggregate_kind TEXT NOT NULL CHECK (source_aggregate_kind = 'revenue_arrangement'),
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    last_source_version BIGINT NOT NULL CHECK (last_source_version >= source_aggregate_version),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    allocated_minor BIGINT NOT NULL CHECK (allocated_minor > 0),
    recognized_minor BIGINT NOT NULL CHECK (
        recognized_minor >= 0 AND recognized_minor <= allocated_minor
    ),
    schedule_status TEXT NOT NULL CHECK (schedule_status IN ('active', 'completed')),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (updated_at >= created_at),
    CHECK (
        (schedule_status = 'completed' AND recognized_minor = allocated_minor)
        OR (schedule_status = 'active' AND recognized_minor < allocated_minor)
    )
);

CREATE INDEX finance_revenue_schedule_scope_customer_status
    ON erpx_finance_write.revenue_schedule_guards
        (scope_key, customer_id, schedule_status, updated_at DESC, schedule_id DESC);
