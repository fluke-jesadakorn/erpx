-- Finance-owned AP authority. Customer invoice/AR tables are intentionally
-- untouched: vendor invoices, payables, and payments have independent guards.

CREATE TABLE erpx_finance_write.procurement_match_evidence_inbox (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    purchase_order_id UUID NOT NULL,
    purchase_order_version BIGINT NOT NULL CHECK (purchase_order_version > 0),
    receipt_id UUID NOT NULL,
    receipt_version BIGINT NOT NULL CHECK (receipt_version > 0),
    scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    supplier_id UUID NOT NULL,
    evidence_document JSONB NOT NULL,
    source_event_position BIGINT NOT NULL CHECK (source_event_position > 0),
    captured_at TIMESTAMPTZ NOT NULL,
    received_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (
        holding_id,purchase_order_id,purchase_order_version,receipt_id,receipt_version
    )
);

CREATE TABLE erpx_finance_write.vendor_invoice_guards (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    vendor_invoice_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    invoice_number TEXT NOT NULL CHECK (octet_length(invoice_number) BETWEEN 1 AND 128),
    supplier_id UUID NOT NULL,
    purchase_order_id UUID NOT NULL,
    purchase_order_version BIGINT NOT NULL CHECK (purchase_order_version > 0),
    receipt_id UUID NOT NULL,
    receipt_version BIGINT NOT NULL CHECK (receipt_version > 0),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    net_minor BIGINT NOT NULL CHECK (net_minor > 0),
    tax_minor BIGINT NOT NULL CHECK (tax_minor >= 0),
    total_minor BIGINT NOT NULL CHECK (total_minor = net_minor + tax_minor),
    payable_id UUID,
    due_on DATE NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('draft','posted')),
    prepared_by TEXT NOT NULL,
    posted_by TEXT,
    posted_at TIMESTAMPTZ,
    evidence_document JSONB NOT NULL,
    aggregate_document JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,vendor_invoice_id),
    UNIQUE (holding_id,scope_key,supplier_id,invoice_number),
    UNIQUE (
        holding_id,vendor_invoice_id,aggregate_version,purchase_order_id,
        purchase_order_version,receipt_id,receipt_version
    ),
    CHECK (updated_at >= created_at),
    CHECK (
        (status='draft' AND payable_id IS NULL AND posted_by IS NULL AND posted_at IS NULL)
        OR (status='posted' AND payable_id IS NOT NULL AND posted_by IS NOT NULL
            AND posted_at IS NOT NULL AND posted_by<>prepared_by)
    ),
    FOREIGN KEY (
        holding_id,purchase_order_id,purchase_order_version,receipt_id,receipt_version
    ) REFERENCES erpx_finance_write.procurement_match_evidence_inbox (
        holding_id,purchase_order_id,purchase_order_version,receipt_id,receipt_version
    )
);

CREATE TABLE erpx_finance_write.vendor_invoice_line_guards (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    vendor_invoice_id UUID NOT NULL,
    line_id UUID NOT NULL,
    purchase_order_line_id UUID NOT NULL,
    receipt_line_id UUID NOT NULL,
    quantity_units BIGINT NOT NULL CHECK (quantity_units > 0),
    quantity_scale SMALLINT NOT NULL CHECK (quantity_scale BETWEEN 0 AND 9),
    amount_minor BIGINT NOT NULL CHECK (amount_minor > 0),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    line_document JSONB NOT NULL,
    PRIMARY KEY (holding_id,vendor_invoice_id,line_id),
    UNIQUE (holding_id,vendor_invoice_id,purchase_order_line_id,receipt_line_id),
    FOREIGN KEY (holding_id,vendor_invoice_id)
        REFERENCES erpx_finance_write.vendor_invoice_guards(holding_id,vendor_invoice_id)
        ON DELETE CASCADE
);

CREATE TABLE erpx_finance_write.payable_guards (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    payable_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    vendor_invoice_id UUID NOT NULL,
    vendor_invoice_version BIGINT NOT NULL CHECK (vendor_invoice_version > 0),
    supplier_id UUID NOT NULL,
    amount_minor BIGINT NOT NULL CHECK (amount_minor > 0),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    payable_document JSONB NOT NULL,
    posted_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,payable_id),
    UNIQUE (holding_id,vendor_invoice_id),
    FOREIGN KEY (holding_id,vendor_invoice_id)
        REFERENCES erpx_finance_write.vendor_invoice_guards(holding_id,vendor_invoice_id)
);

ALTER TABLE erpx_finance_write.vendor_invoice_guards
    ADD CONSTRAINT finance_vendor_invoice_payable_binding
    FOREIGN KEY (holding_id,payable_id)
    REFERENCES erpx_finance_write.payable_guards(holding_id,payable_id)
    DEFERRABLE INITIALLY DEFERRED;

CREATE TABLE erpx_finance_write.payment_guards (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    payment_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    payment_code TEXT NOT NULL CHECK (octet_length(payment_code) BETWEEN 1 AND 128),
    supplier_id UUID NOT NULL,
    amount_minor BIGINT NOT NULL CHECK (amount_minor > 0),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    status TEXT NOT NULL CHECK (status IN ('draft','released')),
    prepared_by TEXT NOT NULL,
    released_by TEXT,
    released_at TIMESTAMPTZ,
    aggregate_document JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,payment_id),
    UNIQUE (holding_id,scope_key,payment_code),
    CHECK (updated_at >= created_at),
    CHECK (
        (status='draft' AND released_by IS NULL AND released_at IS NULL)
        OR (status='released' AND released_by IS NOT NULL AND released_at IS NOT NULL
            AND released_by<>prepared_by)
    )
);

CREATE TABLE erpx_finance_write.payment_allocation_guards (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    payment_id UUID NOT NULL,
    payable_id UUID NOT NULL,
    payable_version BIGINT NOT NULL CHECK (payable_version > 0),
    amount_minor BIGINT NOT NULL CHECK (amount_minor > 0),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    allocation_document JSONB NOT NULL,
    PRIMARY KEY (holding_id,payment_id,payable_id),
    FOREIGN KEY (holding_id,payment_id)
        REFERENCES erpx_finance_write.payment_guards(holding_id,payment_id) ON DELETE CASCADE,
    FOREIGN KEY (holding_id,payable_id)
        REFERENCES erpx_finance_write.payable_guards(holding_id,payable_id)
);

CREATE TABLE erpx_finance_write.payables_command_receipts (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    idempotency_key TEXT NOT NULL,
    command_id UUID NOT NULL,
    request_hash BYTEA NOT NULL CHECK (octet_length(request_hash)=32),
    aggregate_type TEXT NOT NULL CHECK (aggregate_type IN ('vendor_invoice','payment')),
    aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    correlation_id UUID NOT NULL,
    receipt_document JSONB NOT NULL,
    committed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,idempotency_key),
    UNIQUE (holding_id,command_id)
);

CREATE TABLE erpx_finance_write.payables_audit (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    audit_id UUID NOT NULL,
    command_id UUID NOT NULL,
    aggregate_type TEXT NOT NULL CHECK (aggregate_type IN ('vendor_invoice','payment')),
    aggregate_id UUID NOT NULL,
    action TEXT NOT NULL,
    actor_principal_ref TEXT NOT NULL,
    actor_session_id UUID,
    authority_scope JSONB NOT NULL,
    correlation_id UUID NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,audit_id),
    UNIQUE (holding_id,command_id)
);

CREATE FUNCTION erpx_finance_write.enforce_vendor_invoice_transition()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.aggregate_version<>OLD.aggregate_version+1
       OR NEW.scope<>OLD.scope OR NEW.scope_key<>OLD.scope_key
       OR NEW.invoice_number<>OLD.invoice_number OR NEW.supplier_id<>OLD.supplier_id
       OR NEW.purchase_order_id<>OLD.purchase_order_id
       OR NEW.purchase_order_version<>OLD.purchase_order_version
       OR NEW.receipt_id<>OLD.receipt_id OR NEW.receipt_version<>OLD.receipt_version
       OR NEW.currency<>OLD.currency OR NEW.net_minor<>OLD.net_minor
       OR NEW.tax_minor<>OLD.tax_minor OR NEW.total_minor<>OLD.total_minor
       OR NEW.due_on<>OLD.due_on OR NEW.prepared_by<>OLD.prepared_by
       OR NOT (OLD.status='draft' AND NEW.status='posted')
    THEN
        RAISE EXCEPTION 'invalid Finance vendor invoice transition' USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END;
$$;

CREATE FUNCTION erpx_finance_write.enforce_payment_transition()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.aggregate_version<>OLD.aggregate_version+1
       OR NEW.scope<>OLD.scope OR NEW.scope_key<>OLD.scope_key
       OR NEW.payment_code<>OLD.payment_code OR NEW.supplier_id<>OLD.supplier_id
       OR NEW.amount_minor<>OLD.amount_minor OR NEW.currency<>OLD.currency
       OR NEW.prepared_by<>OLD.prepared_by
       OR NOT (OLD.status='draft' AND NEW.status='released')
    THEN
        RAISE EXCEPTION 'invalid Finance payment transition' USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER finance_vendor_invoice_transition
    BEFORE UPDATE ON erpx_finance_write.vendor_invoice_guards
    FOR EACH ROW EXECUTE FUNCTION erpx_finance_write.enforce_vendor_invoice_transition();
CREATE TRIGGER finance_payment_transition
    BEFORE UPDATE ON erpx_finance_write.payment_guards
    FOR EACH ROW EXECUTE FUNCTION erpx_finance_write.enforce_payment_transition();

CREATE FUNCTION erpx_finance_write.reject_payables_evidence_mutation()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'Finance payables evidence is append-only';
END;
$$;

CREATE TRIGGER finance_match_evidence_append_only
    BEFORE UPDATE OR DELETE ON erpx_finance_write.procurement_match_evidence_inbox
    FOR EACH ROW EXECUTE FUNCTION erpx_finance_write.reject_payables_evidence_mutation();
CREATE TRIGGER finance_payable_append_only
    BEFORE UPDATE OR DELETE ON erpx_finance_write.payable_guards
    FOR EACH ROW EXECUTE FUNCTION erpx_finance_write.reject_payables_evidence_mutation();
CREATE TRIGGER finance_payables_receipt_append_only
    BEFORE UPDATE OR DELETE ON erpx_finance_write.payables_command_receipts
    FOR EACH ROW EXECUTE FUNCTION erpx_finance_write.reject_payables_evidence_mutation();
CREATE TRIGGER finance_payables_audit_append_only
    BEFORE UPDATE OR DELETE ON erpx_finance_write.payables_audit
    FOR EACH ROW EXECUTE FUNCTION erpx_finance_write.reject_payables_evidence_mutation();

DO $$
DECLARE
    guarded REGCLASS;
    policy_name TEXT;
BEGIN
    FOREACH guarded IN ARRAY ARRAY[
        'erpx_finance_write.procurement_match_evidence_inbox'::REGCLASS,
        'erpx_finance_write.vendor_invoice_guards'::REGCLASS,
        'erpx_finance_write.vendor_invoice_line_guards'::REGCLASS,
        'erpx_finance_write.payable_guards'::REGCLASS,
        'erpx_finance_write.payment_guards'::REGCLASS,
        'erpx_finance_write.payment_allocation_guards'::REGCLASS,
        'erpx_finance_write.payables_command_receipts'::REGCLASS,
        'erpx_finance_write.payables_audit'::REGCLASS
    ] LOOP
        EXECUTE format('ALTER TABLE %s ENABLE ROW LEVEL SECURITY',guarded);
        EXECUTE format('ALTER TABLE %s FORCE ROW LEVEL SECURITY',guarded);
        policy_name := replace(guarded::TEXT,'.','_')||'_holding_isolation';
        EXECUTE format(
            'CREATE POLICY %I ON %s USING (holding_id=erpx_cqrs_write.current_holding_id()) WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id())',
            policy_name,guarded
        );
    END LOOP;
END;
$$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM erpx_meta.schema_manifest
        WHERE singleton=TRUE AND schema_revision=2 AND manifest_version=1
          AND database_role='holding_write' AND migration_set='holding-write'
    ) THEN
        RAISE EXCEPTION 'holding-write current manifest is not current';
    END IF;
END;
$$;
