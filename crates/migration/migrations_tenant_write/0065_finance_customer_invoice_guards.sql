-- Finance-owned customer invoices created from immutable Sales billing facts.
-- Migration 0081 promotes typed billing and dimension columns to authority.
-- Posting is maker-checker and customer AR is created only from the resulting
-- finance.customer_invoice_posted fact.

CREATE TABLE erpx_finance_write.customer_invoice_guards (
    invoice_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    invoice_code TEXT NOT NULL,
    customer_id UUID NOT NULL,
    billing_event_id UUID NOT NULL UNIQUE,
    source_context TEXT NOT NULL CHECK (source_context = 'sales'),
    source_aggregate_kind TEXT NOT NULL CHECK (source_aggregate_kind = 'revenue_arrangement'),
    source_aggregate_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    net_minor BIGINT NOT NULL CHECK (net_minor > 0),
    tax_minor BIGINT NOT NULL CHECK (tax_minor >= 0),
    receivable_minor BIGINT NOT NULL CHECK (receivable_minor = net_minor + tax_minor),
    eligible_on DATE NOT NULL,
    due_on DATE NOT NULL CHECK (due_on >= eligible_on),
    dimensions JSONB NOT NULL,
    prepared_by TEXT NOT NULL,
    posted_by TEXT,
    posted_at TIMESTAMPTZ,
    invoice_status TEXT NOT NULL CHECK (invoice_status IN ('draft', 'posted', 'cancelled')),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (updated_at >= created_at),
    CHECK (
        (invoice_status = 'posted' AND posted_by IS NOT NULL AND posted_at IS NOT NULL
         AND posted_by <> prepared_by)
        OR (invoice_status <> 'posted' AND posted_by IS NULL AND posted_at IS NULL)
    ),
    UNIQUE (scope_key, invoice_code)
);

CREATE INDEX finance_customer_invoice_scope_customer_status
    ON erpx_finance_write.customer_invoice_guards
        (scope_key, customer_id, invoice_status, updated_at DESC, invoice_id DESC);
