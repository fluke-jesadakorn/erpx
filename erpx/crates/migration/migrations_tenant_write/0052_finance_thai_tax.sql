-- Explicit Thai statutory tax facts linked to immutable posted journal lines.
-- The fact and its outbox event are authoritative; reports are rebuildable.
CREATE SCHEMA IF NOT EXISTS erpx_tenant;

CREATE TABLE IF NOT EXISTS erpx_tenant.finance_thai_tax_facts (
    fact_id UUID PRIMARY KEY,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    journal_id UUID NOT NULL,
    line_id UUID NOT NULL,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    tax_date DATE NOT NULL,
    tax_period CHAR(7) NOT NULL CHECK (tax_period ~ '^[0-9]{4}-[0-9]{2}$'),
    kind TEXT NOT NULL CHECK (kind IN ('output_vat', 'input_vat', 'withholding_payable')),
    currency CHAR(3) NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    base_minor BIGINT NOT NULL CHECK (base_minor > 0),
    tax_minor BIGINT NOT NULL CHECK (tax_minor >= 0),
    tax_rate_basis_points INTEGER NOT NULL CHECK (tax_rate_basis_points BETWEEN 0 AND 10000),
    tax_identifier TEXT NOT NULL CHECK (btrim(tax_identifier) <> '' AND char_length(tax_identifier) <= 255),
    context JSONB NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS erpx_tenant.finance_thai_tax_command_idempotency (
    idempotency_key TEXT PRIMARY KEY CHECK (btrim(idempotency_key) <> ''),
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    request_hash TEXT NOT NULL CHECK (length(request_hash) = 64),
    fact_id UUID NOT NULL REFERENCES erpx_tenant.finance_thai_tax_facts(fact_id) ON DELETE RESTRICT,
    response JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS finance_thai_tax_facts_report_idx
    ON erpx_tenant.finance_thai_tax_facts
       (holding_ref, company_id, tax_period, tax_date, fact_id);
CREATE INDEX IF NOT EXISTS finance_thai_tax_facts_journal_idx
    ON erpx_tenant.finance_thai_tax_facts
       (holding_ref, journal_id, line_id);

ALTER TABLE erpx_tenant.finance_thai_tax_facts ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_thai_tax_facts FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_thai_tax_command_idempotency ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_thai_tax_command_idempotency FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_finance_thai_tax_facts
    ON erpx_tenant.finance_thai_tax_facts;
CREATE POLICY tenant_scope_finance_thai_tax_facts
    ON erpx_tenant.finance_thai_tax_facts
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_finance_thai_tax_idempotency
    ON erpx_tenant.finance_thai_tax_command_idempotency;
CREATE POLICY tenant_scope_finance_thai_tax_idempotency
    ON erpx_tenant.finance_thai_tax_command_idempotency
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

CREATE OR REPLACE FUNCTION erpx_tenant.reject_finance_thai_tax_fact_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        RETURN NEW;
    END IF;
    RAISE EXCEPTION 'Thai statutory tax facts are append-only';
END;
$$;

DROP TRIGGER IF EXISTS finance_thai_tax_facts_append_only
    ON erpx_tenant.finance_thai_tax_facts;
CREATE TRIGGER finance_thai_tax_facts_append_only
    BEFORE UPDATE OR DELETE ON erpx_tenant.finance_thai_tax_facts
    FOR EACH ROW
    EXECUTE FUNCTION erpx_tenant.reject_finance_thai_tax_fact_mutation();
