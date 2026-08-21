-- Durable Procurement-to-Payment process state. Source receipts and released
-- purchase-order facts are holding-local and append-only; the process cursor
-- advances only after every deterministic owner command for an event succeeds.

CREATE SCHEMA IF NOT EXISTS erpx_procure_to_pay_write;

CREATE TABLE erpx_procure_to_pay_write.process_cursor (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    process_name TEXT NOT NULL CHECK (process_name='procure-to-pay-v1'),
    global_sequence BIGINT NOT NULL CHECK (global_sequence >= 0),
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,process_name)
);

CREATE TABLE erpx_procure_to_pay_write.source_receipts (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    event_id UUID NOT NULL,
    global_sequence BIGINT NOT NULL CHECK (global_sequence > 0),
    event_type TEXT NOT NULL CHECK (octet_length(event_type) BETWEEN 1 AND 128),
    processed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,event_id),
    UNIQUE (holding_id,global_sequence)
);

CREATE TABLE erpx_procure_to_pay_write.purchase_order_facts (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    purchase_order_id UUID NOT NULL,
    purchase_order_version BIGINT NOT NULL CHECK (purchase_order_version > 0),
    active_scope JSONB NOT NULL,
    supplier_id UUID NOT NULL,
    fact_document JSONB NOT NULL,
    source_event_id UUID NOT NULL,
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    captured_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,purchase_order_id,purchase_order_version),
    UNIQUE (holding_id,source_event_id)
);

CREATE FUNCTION erpx_procure_to_pay_write.reject_append_only_mutation()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'Procure-to-Pay evidence is append-only';
END;
$$;

CREATE TRIGGER procure_to_pay_source_receipts_append_only
    BEFORE UPDATE OR DELETE ON erpx_procure_to_pay_write.source_receipts
    FOR EACH ROW EXECUTE FUNCTION erpx_procure_to_pay_write.reject_append_only_mutation();
CREATE TRIGGER procure_to_pay_purchase_order_facts_append_only
    BEFORE UPDATE OR DELETE ON erpx_procure_to_pay_write.purchase_order_facts
    FOR EACH ROW EXECUTE FUNCTION erpx_procure_to_pay_write.reject_append_only_mutation();

DO $$
DECLARE
    guarded REGCLASS;
    policy_name TEXT;
BEGIN
    FOREACH guarded IN ARRAY ARRAY[
        'erpx_procure_to_pay_write.process_cursor'::REGCLASS,
        'erpx_procure_to_pay_write.source_receipts'::REGCLASS,
        'erpx_procure_to_pay_write.purchase_order_facts'::REGCLASS
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

UPDATE erpx_meta.schema_manifest
SET installed_at=now()
WHERE singleton=TRUE AND schema_revision=2 AND manifest_version=1
  AND database_role='holding_write' AND migration_set='holding-write';

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
