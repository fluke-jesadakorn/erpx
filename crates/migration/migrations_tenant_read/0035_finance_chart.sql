-- Rebuildable, normalized tenant-read Chart of Accounts projection.
-- The tenant-write finance_chart_* tables and outbox are authoritative.

CREATE SCHEMA IF NOT EXISTS erpx_tenant;

CREATE TABLE IF NOT EXISTS erpx_tenant.finance_chart_read_rows (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    chart_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    chart_name TEXT NOT NULL CHECK (btrim(chart_name) <> '' AND char_length(chart_name) <= 255),
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    source_event_id UUID NOT NULL,
    source_contract_version INTEGER NOT NULL CHECK (source_contract_version > 0),
    source_event_version INTEGER NOT NULL CHECK (source_event_version > 0),
    source_actor_ref TEXT NOT NULL CHECK (btrim(source_actor_ref) <> '' AND char_length(source_actor_ref) <= 255),
    source_actor_kind TEXT NOT NULL CHECK (source_actor_kind IN ('platform_user', 'tenant_user', 'service', 'system_worker')),
    source_correlation_id UUID NOT NULL,
    source_causation_id UUID,
    source_idempotency_key TEXT NOT NULL CHECK (btrim(source_idempotency_key) <> '' AND char_length(source_idempotency_key) <= 512),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, chart_id)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.finance_chart_account_read_rows (
    holding_ref TEXT NOT NULL,
    chart_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    account_id UUID NOT NULL,
    code TEXT NOT NULL CHECK (btrim(code) <> '' AND char_length(code) <= 64),
    account_name TEXT NOT NULL CHECK (btrim(account_name) <> '' AND char_length(account_name) <= 255),
    category TEXT NOT NULL CHECK (category IN ('asset', 'liability', 'equity', 'revenue', 'expense')),
    normal_balance TEXT NOT NULL CHECK (normal_balance IN ('debit', 'credit')),
    parent_account_id UUID,
    status TEXT NOT NULL CHECK (status IN ('active', 'inactive')),
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    source_event_id UUID NOT NULL,
    source_contract_version INTEGER NOT NULL CHECK (source_contract_version > 0),
    source_event_version INTEGER NOT NULL CHECK (source_event_version > 0),
    source_actor_ref TEXT NOT NULL CHECK (btrim(source_actor_ref) <> '' AND char_length(source_actor_ref) <= 255),
    source_actor_kind TEXT NOT NULL CHECK (source_actor_kind IN ('platform_user', 'tenant_user', 'service', 'system_worker')),
    source_correlation_id UUID NOT NULL,
    source_causation_id UUID,
    source_idempotency_key TEXT NOT NULL CHECK (btrim(source_idempotency_key) <> '' AND char_length(source_idempotency_key) <= 512),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, chart_id, account_id),
    UNIQUE (holding_ref, chart_id, code),
    FOREIGN KEY (holding_ref, chart_id)
        REFERENCES erpx_tenant.finance_chart_read_rows (holding_ref, chart_id)
        ON DELETE CASCADE,
    CHECK (parent_account_id IS NULL OR parent_account_id <> account_id)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.finance_chart_projection_checkpoints (
    holding_ref TEXT NOT NULL,
    projection_name TEXT NOT NULL CHECK (projection_name = 'finance_chart'),
    aggregate_type TEXT NOT NULL CHECK (aggregate_type = 'finance_chart_of_accounts'),
    aggregate_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    last_event_id UUID NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, projection_name, aggregate_id)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.finance_chart_projection_event_receipts (
    holding_ref TEXT NOT NULL,
    projection_name TEXT NOT NULL CHECK (projection_name = 'finance_chart'),
    event_id UUID NOT NULL,
    aggregate_type TEXT NOT NULL CHECK (aggregate_type = 'finance_chart_of_accounts'),
    aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    event_type TEXT NOT NULL CHECK (event_type = 'finance.chart_of_accounts_saved.v1'),
    source_contract_version INTEGER NOT NULL CHECK (source_contract_version > 0),
    source_event_version INTEGER NOT NULL CHECK (source_event_version > 0),
    source_actor_ref TEXT NOT NULL CHECK (btrim(source_actor_ref) <> '' AND char_length(source_actor_ref) <= 255),
    source_actor_kind TEXT NOT NULL CHECK (source_actor_kind IN ('platform_user', 'tenant_user', 'service', 'system_worker')),
    source_correlation_id UUID NOT NULL,
    source_causation_id UUID,
    source_idempotency_key TEXT NOT NULL CHECK (btrim(source_idempotency_key) <> '' AND char_length(source_idempotency_key) <= 512),
    received_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    processed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, projection_name, event_id)
);

CREATE INDEX IF NOT EXISTS finance_chart_read_scope_idx
    ON erpx_tenant.finance_chart_read_rows
       (holding_ref, holding_id, company_id, projected_at DESC, chart_id);
CREATE INDEX IF NOT EXISTS finance_chart_account_read_scope_idx
    ON erpx_tenant.finance_chart_account_read_rows
       (holding_ref, holding_id, company_id, chart_id, code);
CREATE INDEX IF NOT EXISTS finance_chart_projection_checkpoint_source_idx
    ON erpx_tenant.finance_chart_projection_checkpoints
       (holding_ref, aggregate_id, source_version);
CREATE INDEX IF NOT EXISTS finance_chart_projection_receipt_source_idx
    ON erpx_tenant.finance_chart_projection_event_receipts
       (holding_ref, aggregate_id, aggregate_version);

ALTER TABLE erpx_tenant.finance_chart_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_chart_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_chart_account_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_chart_account_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_chart_projection_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_chart_projection_checkpoints FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_chart_projection_event_receipts ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_chart_projection_event_receipts FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_finance_chart_read_rows ON erpx_tenant.finance_chart_read_rows;
CREATE POLICY tenant_scope_finance_chart_read_rows ON erpx_tenant.finance_chart_read_rows
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_finance_chart_account_read_rows ON erpx_tenant.finance_chart_account_read_rows;
CREATE POLICY tenant_scope_finance_chart_account_read_rows ON erpx_tenant.finance_chart_account_read_rows
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_finance_chart_projection_checkpoints ON erpx_tenant.finance_chart_projection_checkpoints;
CREATE POLICY tenant_scope_finance_chart_projection_checkpoints ON erpx_tenant.finance_chart_projection_checkpoints
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_finance_chart_projection_event_receipts ON erpx_tenant.finance_chart_projection_event_receipts;
CREATE POLICY tenant_scope_finance_chart_projection_event_receipts ON erpx_tenant.finance_chart_projection_event_receipts
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
