-- Disposable Finance banking read model for one routed holding.
-- Source rows remain authoritative in tenant_write/0028_finance_banking.sql.
CREATE SCHEMA IF NOT EXISTS erpx_tenant;

CREATE TABLE IF NOT EXISTS erpx_tenant.finance_banking_read_rows (
    aggregate_type TEXT NOT NULL CHECK (aggregate_type IN (
        'finance_bank_account',
        'finance_bank_statement_import',
        'finance_bank_reconciliation'
    )),
    aggregate_id UUID NOT NULL,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    payload JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (aggregate_type, aggregate_id)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.finance_banking_projection_checkpoints (
    aggregate_type TEXT NOT NULL CHECK (aggregate_type IN (
        'finance_bank_account',
        'finance_bank_statement_import',
        'finance_bank_reconciliation'
    )),
    aggregate_id UUID NOT NULL,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    last_event_id UUID NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (aggregate_type, aggregate_id)
);

CREATE INDEX IF NOT EXISTS finance_banking_read_rows_scope_idx
    ON erpx_tenant.finance_banking_read_rows
       (holding_ref, company_id, aggregate_type, source_version, aggregate_id);

CREATE INDEX IF NOT EXISTS finance_banking_projection_checkpoints_scope_idx
    ON erpx_tenant.finance_banking_projection_checkpoints
       (holding_ref, aggregate_type, source_version, aggregate_id);

ALTER TABLE erpx_tenant.finance_banking_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_banking_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_banking_projection_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_banking_projection_checkpoints FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_finance_banking_read_rows
    ON erpx_tenant.finance_banking_read_rows;
CREATE POLICY tenant_scope_finance_banking_read_rows
    ON erpx_tenant.finance_banking_read_rows
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

DROP POLICY IF EXISTS tenant_scope_finance_banking_projection_checkpoints
    ON erpx_tenant.finance_banking_projection_checkpoints;
CREATE POLICY tenant_scope_finance_banking_projection_checkpoints
    ON erpx_tenant.finance_banking_projection_checkpoints
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
