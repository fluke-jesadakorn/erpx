-- Canonical control read DDL: migrations 0015 and 0016.
CREATE SCHEMA IF NOT EXISTS erpx_customer_authority_read;

CREATE TABLE erpx_customer_authority_read.customer_projection (
    projection_name TEXT NOT NULL DEFAULT 'customer_authority.customers',
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    customer_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL,
    source_global_sequence BIGINT NOT NULL,
    customer_code TEXT NOT NULL,
    trading_name TEXT NOT NULL,
    legal_name TEXT,
    country TEXT NOT NULL,
    customer_status TEXT NOT NULL,
    primary_authority JSONB NOT NULL,
    primary_scope_key TEXT NOT NULL,
    merged_into_customer_id UUID,
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, customer_id)
);

CREATE TABLE erpx_customer_authority_read.authority_link_projection (
    projection_name TEXT NOT NULL DEFAULT 'customer_authority.customers',
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    customer_id UUID NOT NULL,
    scope_key TEXT NOT NULL,
    authority_scope JSONB NOT NULL,
    is_primary BOOLEAN NOT NULL,
    source_aggregate_version BIGINT NOT NULL,
    source_global_sequence BIGINT NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, customer_id, scope_key)
);
