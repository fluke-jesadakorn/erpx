-- Finance read state is a disposable projection of immutable posted journals.
-- It is intentionally separate from the tenant write tables and is rebuilt by
-- the worker from the write-side journal/outbox history.
CREATE SCHEMA IF NOT EXISTS erpx_tenant;

CREATE TABLE IF NOT EXISTS erpx_tenant.finance_posted_journals (
    journal_id UUID PRIMARY KEY,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    period_id UUID NOT NULL,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    posted_at TIMESTAMPTZ NOT NULL,
    payload JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS finance_read_journals_period_idx
    ON erpx_tenant.finance_posted_journals
       (holding_ref, company_id, period_id, posted_at, journal_id);

ALTER TABLE erpx_tenant.finance_posted_journals ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_posted_journals FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_finance_read_journals
    ON erpx_tenant.finance_posted_journals;
CREATE POLICY tenant_scope_finance_read_journals
    ON erpx_tenant.finance_posted_journals
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
