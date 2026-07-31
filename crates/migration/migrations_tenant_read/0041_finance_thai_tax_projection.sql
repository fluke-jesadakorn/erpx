-- Rebuildable Thai tax report projection. No journal payload or command
-- context is copied into the read database.
CREATE SCHEMA IF NOT EXISTS erpx_tenant;

CREATE TABLE IF NOT EXISTS erpx_tenant.finance_thai_tax_fact_read_rows (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    fact_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    journal_id UUID NOT NULL,
    line_id UUID NOT NULL,
    tax_date DATE NOT NULL,
    tax_period CHAR(7) NOT NULL CHECK (tax_period ~ '^[0-9]{4}-[0-9]{2}$'),
    kind TEXT NOT NULL CHECK (kind IN ('output_vat', 'input_vat', 'withholding_payable')),
    currency CHAR(3) NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    base_minor BIGINT NOT NULL CHECK (base_minor > 0),
    tax_minor BIGINT NOT NULL CHECK (tax_minor >= 0),
    tax_rate_basis_points INTEGER NOT NULL CHECK (tax_rate_basis_points BETWEEN 0 AND 10000),
    tax_identifier TEXT NOT NULL CHECK (btrim(tax_identifier) <> '' AND char_length(tax_identifier) <= 255),
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    source_event_id UUID NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, fact_id)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.finance_thai_tax_projection_checkpoints (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    fact_id UUID NOT NULL,
    last_event_id UUID NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, fact_id)
);

CREATE INDEX IF NOT EXISTS finance_thai_tax_fact_read_report_idx
    ON erpx_tenant.finance_thai_tax_fact_read_rows
       (holding_ref, company_id, tax_period, tax_date, fact_id);

ALTER TABLE erpx_tenant.finance_thai_tax_fact_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_thai_tax_fact_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_thai_tax_projection_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_thai_tax_projection_checkpoints FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_finance_thai_tax_fact_read_rows
    ON erpx_tenant.finance_thai_tax_fact_read_rows;
CREATE POLICY tenant_scope_finance_thai_tax_fact_read_rows
    ON erpx_tenant.finance_thai_tax_fact_read_rows
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_finance_thai_tax_projection_checkpoints
    ON erpx_tenant.finance_thai_tax_projection_checkpoints;
CREATE POLICY tenant_scope_finance_thai_tax_projection_checkpoints
    ON erpx_tenant.finance_thai_tax_projection_checkpoints
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
