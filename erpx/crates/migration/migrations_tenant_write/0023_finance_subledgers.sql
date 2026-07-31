-- Authoritative Finance AP/AR and cash-control state for one routed holding.
-- The aggregate payload remains the domain-owned source of truth; the
-- specialized columns are routing and query guards only.
CREATE SCHEMA IF NOT EXISTS erpx_tenant;

CREATE TABLE IF NOT EXISTS erpx_tenant.finance_subledger_aggregates (
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
    reference TEXT NOT NULL CHECK (btrim(reference) <> ''),
    version BIGINT NOT NULL CHECK (version > 0),
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    payload JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (aggregate_type, aggregate_id),
    UNIQUE (holding_ref, aggregate_type, idempotency_key)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.finance_subledger_command_idempotency (
    idempotency_key TEXT PRIMARY KEY CHECK (btrim(idempotency_key) <> ''),
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    operation TEXT NOT NULL CHECK (btrim(operation) <> ''),
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    request_hash TEXT NOT NULL CHECK (length(request_hash) = 64),
    response JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS finance_subledger_aggregates_scope_idx
    ON erpx_tenant.finance_subledger_aggregates
        (holding_ref, company_id, branch_id, aggregate_type, updated_at DESC);

ALTER TABLE erpx_tenant.finance_subledger_aggregates ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_subledger_aggregates FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_subledger_command_idempotency ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_subledger_command_idempotency FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_finance_subledger_aggregates
    ON erpx_tenant.finance_subledger_aggregates;
CREATE POLICY tenant_scope_finance_subledger_aggregates
    ON erpx_tenant.finance_subledger_aggregates
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

DROP POLICY IF EXISTS tenant_scope_finance_subledger_idempotency
    ON erpx_tenant.finance_subledger_command_idempotency;
CREATE POLICY tenant_scope_finance_subledger_idempotency
    ON erpx_tenant.finance_subledger_command_idempotency
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
