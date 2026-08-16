-- Disposable Finance customer-invoice workbench projection.

CREATE TABLE erpx_finance_read.customer_invoice_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_tenant_id(),
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
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, invoice_id),
    FOREIGN KEY (tenant_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (tenant_id, projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (tenant_id, generation, billing_event_id),
    UNIQUE (tenant_id, generation, scope_key, invoice_code)
);

CREATE INDEX finance_read_customer_invoice_scope_customer_status
    ON erpx_finance_read.customer_invoice_projection
        (tenant_id, generation, scope_key, customer_id, invoice_status,
         aggregate_updated_at DESC, invoice_id DESC);

DO $$
DECLARE schema_name TEXT; table_name TEXT;
BEGIN
    FOR schema_name, table_name IN SELECT * FROM (VALUES
        ('erpx_finance_read','customer_invoice_projection')
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
