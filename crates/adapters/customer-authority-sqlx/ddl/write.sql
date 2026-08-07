-- Canonical control write DDL: migrations 0075 and 0076.
-- This evidence file intentionally mirrors only the context-specific guards.
CREATE SCHEMA IF NOT EXISTS erpx_customer_authority_write;

CREATE TABLE erpx_customer_authority_write.account_guards (
    customer_id UUID PRIMARY KEY,
    customer_code TEXT NOT NULL UNIQUE,
    primary_scope_key TEXT NOT NULL,
    primary_holding_id UUID NOT NULL,
    primary_company_id UUID NOT NULL,
    customer_status TEXT NOT NULL,
    merged_into_customer_id UUID,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE erpx_customer_authority_write.authority_link_guards (
    customer_id UUID NOT NULL,
    scope_key TEXT NOT NULL,
    scope_kind TEXT NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID,
    branch_id UUID,
    is_primary BOOLEAN NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (customer_id, scope_key)
);
