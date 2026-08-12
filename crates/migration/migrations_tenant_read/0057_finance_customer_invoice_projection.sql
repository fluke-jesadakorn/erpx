-- Disposable Finance customer-invoice workbench projection.

CREATE TABLE erpx_finance_read.customer_invoice_projection (
    projection_name TEXT NOT NULL DEFAULT 'finance.customer_invoices'
        CHECK (projection_name = 'finance.customer_invoices'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    invoice_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    invoice_code TEXT NOT NULL,
    customer_id UUID NOT NULL,
    billing_event_id UUID NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    receivable_minor BIGINT NOT NULL CHECK (receivable_minor > 0),
    due_on DATE NOT NULL,
    invoice_status TEXT NOT NULL CHECK (invoice_status IN ('draft', 'posted', 'cancelled')),
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, invoice_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (generation, billing_event_id),
    UNIQUE (generation, scope_key, invoice_code)
);

CREATE INDEX finance_read_customer_invoice_scope_customer_status
    ON erpx_finance_read.customer_invoice_projection
        (generation, scope_key, customer_id, invoice_status,
         aggregate_updated_at DESC, invoice_id DESC);
