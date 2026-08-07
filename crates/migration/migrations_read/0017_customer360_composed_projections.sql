-- Wave 9 composed Customer 360 read model. Customer Authority is read from the
-- control outbox; tenant activity uses one independently fenced cursor per
-- immutable tenant database route.

CREATE TABLE erpx_cqrs_read.projection_source_checkpoints (
    projection_name TEXT NOT NULL,
    generation UUID NOT NULL,
    source_key TEXT NOT NULL CHECK (octet_length(source_key) BETWEEN 1 AND 255),
    source_sequence BIGINT NOT NULL DEFAULT 0 CHECK (source_sequence >= 0),
    rebuild_target_sequence BIGINT CHECK (rebuild_target_sequence >= 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, source_key),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    CHECK (updated_at >= created_at)
);

CREATE INDEX cqrs_control_projection_source_rebuild
    ON erpx_cqrs_read.projection_source_checkpoints
        (projection_name, generation, rebuild_target_sequence, source_sequence);

CREATE SCHEMA IF NOT EXISTS erpx_customer360_read;

CREATE TABLE erpx_customer360_read.customer_profile_projection (
    projection_name TEXT NOT NULL DEFAULT 'customer360.control_customers'
        CHECK (projection_name = 'customer360.control_customers'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    customer_id UUID NOT NULL,
    customer_account_id UUID NOT NULL,
    customer_code TEXT NOT NULL,
    display_name TEXT NOT NULL,
    owning_company_id UUID NOT NULL,
    customer_status TEXT NOT NULL CHECK (customer_status IN (
        'prospect', 'active', 'dormant', 'blocked', 'closed'
    )),
    source_event_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    occurred_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, customer_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (generation, customer_account_id),
    UNIQUE (generation, customer_code)
);

CREATE INDEX customer360_profile_customer
    ON erpx_customer360_read.customer_profile_projection
        (generation, customer_id, projected_at DESC);

CREATE TABLE erpx_customer360_read.subscription_projection (
    projection_name TEXT NOT NULL DEFAULT 'customer360.tenant_activity'
        CHECK (projection_name = 'customer360.tenant_activity'),
    generation UUID NOT NULL,
    source_key TEXT NOT NULL,
    partition_key TEXT NOT NULL,
    tenant_id UUID NOT NULL,
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    customer_id UUID NOT NULL,
    subscription_id UUID NOT NULL,
    plan_id UUID NOT NULL,
    subscription_status TEXT NOT NULL CHECK (subscription_status IN (
        'trial', 'active', 'past_due', 'suspended', 'ended'
    )),
    recurring_amount_minor BIGINT NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    effective_at TIMESTAMPTZ NOT NULL,
    source_event_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, source_key, subscription_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (generation, source_key, partition_key, source_event_id),
    CHECK (source_key = 'tenant:' || tenant_id::text)
);

CREATE INDEX customer360_subscription_customer
    ON erpx_customer360_read.subscription_projection
        (generation, customer_id, source_key, effective_at DESC, subscription_id DESC);

CREATE TABLE erpx_customer360_read.opportunity_projection (
    projection_name TEXT NOT NULL DEFAULT 'customer360.tenant_activity'
        CHECK (projection_name = 'customer360.tenant_activity'),
    generation UUID NOT NULL,
    source_key TEXT NOT NULL,
    partition_key TEXT NOT NULL,
    tenant_id UUID NOT NULL,
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    customer_id UUID NOT NULL,
    opportunity_id UUID NOT NULL,
    opportunity_stage TEXT NOT NULL CHECK (opportunity_stage IN (
        'qualified', 'discovery', 'proposal', 'negotiation', 'won', 'lost'
    )),
    expected_value_minor BIGINT NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    source_event_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, source_key, opportunity_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (generation, source_key, partition_key, source_event_id),
    CHECK (source_key = 'tenant:' || tenant_id::text)
);

CREATE INDEX customer360_opportunity_customer
    ON erpx_customer360_read.opportunity_projection
        (generation, customer_id, source_key, opportunity_id DESC);

CREATE TABLE erpx_customer360_read.sales_order_projection (
    projection_name TEXT NOT NULL DEFAULT 'customer360.tenant_activity'
        CHECK (projection_name = 'customer360.tenant_activity'),
    generation UUID NOT NULL,
    source_key TEXT NOT NULL,
    partition_key TEXT NOT NULL,
    tenant_id UUID NOT NULL,
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    customer_id UUID NOT NULL,
    sales_order_id UUID NOT NULL,
    order_code TEXT NOT NULL,
    order_status TEXT NOT NULL CHECK (order_status IN (
        'confirmed', 'partially_fulfilled', 'fulfilled', 'cancelled'
    )),
    total_minor BIGINT NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    lines JSONB NOT NULL CHECK (
        jsonb_typeof(lines) = 'array' AND jsonb_array_length(lines) > 0
    ),
    source_event_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, source_key, sales_order_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (generation, source_key, partition_key, source_event_id),
    UNIQUE (generation, source_key, order_code),
    CHECK (source_key = 'tenant:' || tenant_id::text)
);

CREATE INDEX customer360_sales_order_customer
    ON erpx_customer360_read.sales_order_projection
        (generation, customer_id, source_key, sales_order_id DESC);

CREATE TABLE erpx_customer360_read.support_case_projection (
    projection_name TEXT NOT NULL DEFAULT 'customer360.tenant_activity'
        CHECK (projection_name = 'customer360.tenant_activity'),
    generation UUID NOT NULL,
    source_key TEXT NOT NULL,
    partition_key TEXT NOT NULL,
    tenant_id UUID NOT NULL,
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    customer_id UUID NOT NULL,
    support_case_id UUID NOT NULL,
    case_code TEXT NOT NULL,
    case_status TEXT NOT NULL CHECK (case_status IN (
        'open', 'pending_customer', 'pending_internal', 'resolved', 'closed'
    )),
    category TEXT NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    source_event_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, source_key, support_case_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (generation, source_key, partition_key, source_event_id),
    UNIQUE (generation, source_key, case_code),
    CHECK (source_key = 'tenant:' || tenant_id::text)
);

CREATE INDEX customer360_support_case_customer
    ON erpx_customer360_read.support_case_projection
        (generation, customer_id, source_key, aggregate_updated_at DESC, support_case_id DESC);
