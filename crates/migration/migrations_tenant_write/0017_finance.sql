-- Finance write-side state is tenant-local and is rebuildable only from the
-- authoritative posted journal rows. All tables are holding-bound and force
-- the transaction-local erpx.holding_ref RLS setting.
CREATE SCHEMA IF NOT EXISTS erpx_tenant;

CREATE TABLE IF NOT EXISTS erpx_tenant.finance_charts (
    chart_id UUID PRIMARY KEY,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    version BIGINT NOT NULL CHECK (version > 0),
    payload JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS erpx_tenant.finance_periods (
    period_id UUID PRIMARY KEY,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    status TEXT NOT NULL CHECK (status IN ('open', 'locked', 'closed')),
    version BIGINT NOT NULL CHECK (version > 0),
    payload JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS erpx_tenant.finance_posted_journals (
    journal_id UUID PRIMARY KEY,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    period_id UUID NOT NULL,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    version BIGINT NOT NULL CHECK (version > 0),
    idempotency_key TEXT NOT NULL UNIQUE CHECK (btrim(idempotency_key) <> ''),
    posted_at TIMESTAMPTZ NOT NULL,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS erpx_tenant.finance_command_idempotency (
    idempotency_key TEXT PRIMARY KEY CHECK (btrim(idempotency_key) <> ''),
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    request_hash TEXT NOT NULL CHECK (length(request_hash) = 64),
    journal_id UUID NOT NULL,
    response JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS finance_charts_company_idx
    ON erpx_tenant.finance_charts (holding_ref, company_id, updated_at DESC);
CREATE INDEX IF NOT EXISTS finance_periods_company_idx
    ON erpx_tenant.finance_periods (holding_ref, company_id, status, updated_at DESC);
CREATE INDEX IF NOT EXISTS finance_journals_period_idx
    ON erpx_tenant.finance_posted_journals (holding_ref, company_id, period_id, posted_at, journal_id);

ALTER TABLE erpx_tenant.finance_charts ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_charts FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_periods ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_periods FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_posted_journals ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_posted_journals FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_command_idempotency ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_command_idempotency FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_finance_charts ON erpx_tenant.finance_charts;
CREATE POLICY tenant_scope_finance_charts ON erpx_tenant.finance_charts
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_finance_periods ON erpx_tenant.finance_periods;
CREATE POLICY tenant_scope_finance_periods ON erpx_tenant.finance_periods
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_finance_journals ON erpx_tenant.finance_posted_journals;
CREATE POLICY tenant_scope_finance_journals ON erpx_tenant.finance_posted_journals
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_finance_idempotency ON erpx_tenant.finance_command_idempotency;
CREATE POLICY tenant_scope_finance_idempotency ON erpx_tenant.finance_command_idempotency
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
