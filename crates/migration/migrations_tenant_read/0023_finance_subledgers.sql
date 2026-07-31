-- Disposable Finance AP/AR and cash-control projection for one routed holding.
CREATE SCHEMA IF NOT EXISTS erpx_tenant;

CREATE TABLE IF NOT EXISTS erpx_tenant.finance_subledger_read_rows (
    aggregate_id UUID NOT NULL,
    aggregate_type TEXT NOT NULL CHECK (aggregate_type IN (
        'payable_invoice', 'receivable_invoice', 'vendor_payment',
        'customer_payment', 'allocation', 'cash_receipt',
        'cash_disbursement', 'cash_reconciliation',
        'vendor_subledger_entry', 'customer_subledger_entry'
    )),
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    company_id UUID NOT NULL,
    branch_id UUID,
    period_id UUID NOT NULL,
    accounting_date DATE NOT NULL,
    reference TEXT NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    payload JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (aggregate_type, aggregate_id)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.finance_subledger_projection_checkpoints (
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    last_event_id UUID NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (aggregate_type, aggregate_id)
);

CREATE INDEX IF NOT EXISTS finance_subledger_read_scope_idx
    ON erpx_tenant.finance_subledger_read_rows
        (holding_ref, company_id, branch_id, aggregate_type, accounting_date DESC);

ALTER TABLE erpx_tenant.finance_subledger_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_subledger_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_subledger_projection_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_subledger_projection_checkpoints FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_finance_subledger_read_rows
    ON erpx_tenant.finance_subledger_read_rows;
CREATE POLICY tenant_scope_finance_subledger_read_rows
    ON erpx_tenant.finance_subledger_read_rows
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

DROP POLICY IF EXISTS tenant_scope_finance_subledger_checkpoints
    ON erpx_tenant.finance_subledger_projection_checkpoints;
CREATE POLICY tenant_scope_finance_subledger_checkpoints
    ON erpx_tenant.finance_subledger_projection_checkpoints
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
