-- Disposable Finance AP projections. Authority remains in holding-write.

CREATE TABLE erpx_finance_read.vendor_invoice_projection (
    holding_id UUID NOT NULL DEFAULT erpx_finance_read.current_holding_id(),
    projection_name TEXT NOT NULL,
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    vendor_invoice_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    supplier_id UUID NOT NULL,
    invoice_number TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('draft','posted')),
    payable_id UUID,
    total_minor BIGINT NOT NULL CHECK (total_minor > 0),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    projection_document JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,projection_name,generation,partition_key,vendor_invoice_id),
    FOREIGN KEY (holding_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations(holding_id,projection_name,generation)
        ON DELETE CASCADE
);

CREATE INDEX finance_vendor_invoice_projection_scope
    ON erpx_finance_read.vendor_invoice_projection
       (holding_id,projection_name,generation,scope_key,status,aggregate_updated_at DESC,vendor_invoice_id);

CREATE TABLE erpx_finance_read.payable_projection (
    holding_id UUID NOT NULL DEFAULT erpx_finance_read.current_holding_id(),
    projection_name TEXT NOT NULL,
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    payable_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    vendor_invoice_id UUID NOT NULL,
    vendor_invoice_version BIGINT NOT NULL CHECK (vendor_invoice_version > 0),
    supplier_id UUID NOT NULL,
    amount_minor BIGINT NOT NULL CHECK (amount_minor > 0),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    projection_document JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,projection_name,generation,partition_key,payable_id),
    FOREIGN KEY (holding_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations(holding_id,projection_name,generation)
        ON DELETE CASCADE
);

CREATE TABLE erpx_finance_read.payment_projection (
    holding_id UUID NOT NULL DEFAULT erpx_finance_read.current_holding_id(),
    projection_name TEXT NOT NULL,
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    payment_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    supplier_id UUID NOT NULL,
    payment_code TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('draft','released')),
    amount_minor BIGINT NOT NULL CHECK (amount_minor > 0),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    projection_document JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,projection_name,generation,partition_key,payment_id),
    FOREIGN KEY (holding_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations(holding_id,projection_name,generation)
        ON DELETE CASCADE
);

CREATE INDEX finance_payment_projection_scope
    ON erpx_finance_read.payment_projection
       (holding_id,projection_name,generation,scope_key,status,aggregate_updated_at DESC,payment_id);

DO $$
DECLARE
    projected REGCLASS;
    policy_name TEXT;
BEGIN
    FOREACH projected IN ARRAY ARRAY[
        'erpx_finance_read.vendor_invoice_projection'::REGCLASS,
        'erpx_finance_read.payable_projection'::REGCLASS,
        'erpx_finance_read.payment_projection'::REGCLASS
    ] LOOP
        EXECUTE format('ALTER TABLE %s ENABLE ROW LEVEL SECURITY',projected);
        EXECUTE format('ALTER TABLE %s FORCE ROW LEVEL SECURITY',projected);
        policy_name := replace(projected::TEXT,'.','_')||'_holding_isolation';
        EXECUTE format(
            'CREATE POLICY %I ON %s USING (holding_id=erpx_finance_read.current_holding_id()) WITH CHECK (holding_id=erpx_finance_read.current_holding_id())',
            policy_name,projected
        );
    END LOOP;
END;
$$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM erpx_meta.schema_manifest
        WHERE singleton=TRUE AND schema_revision=2 AND manifest_version=1
          AND database_role='holding_read' AND migration_set='holding-read'
    ) THEN
        RAISE EXCEPTION 'holding-read current manifest is not current';
    END IF;
END;
$$;
