-- Company-scoped Finance budgets and immutable commitment facts. Actual
-- journal amounts remain owned by finance_posted_journals; this context only
-- owns the approved plan and commitment references used for control checks.
CREATE TABLE IF NOT EXISTS erpx_tenant.finance_budgets (
    budget_id UUID PRIMARY KEY,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    fiscal_year SMALLINT NOT NULL CHECK (fiscal_year BETWEEN 1900 AND 2200),
    currency CHAR(3) NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    status TEXT NOT NULL CHECK (status IN ('draft', 'submitted', 'approved', 'locked', 'closed')),
    version BIGINT NOT NULL CHECK (version > 0),
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    approved_by TEXT,
    approval_limit_minor BIGINT CHECK (approval_limit_minor >= 0),
    payload JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (holding_ref, idempotency_key),
    UNIQUE (holding_ref, budget_id)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.finance_budget_commitments (
    commitment_id UUID PRIMARY KEY,
    budget_id UUID NOT NULL,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    company_id UUID NOT NULL,
    account_id UUID NOT NULL,
    period SMALLINT NOT NULL CHECK (period BETWEEN 1 AND 12),
    amount_minor BIGINT NOT NULL CHECK (amount_minor > 0),
    actual_minor BIGINT NOT NULL CHECK (actual_minor >= 0),
    decision_budget_minor BIGINT NOT NULL CHECK (decision_budget_minor >= 0),
    decision_new_commitment_minor BIGINT NOT NULL CHECK (decision_new_commitment_minor >= 0),
    decision_remaining_minor BIGINT NOT NULL CHECK (decision_remaining_minor >= 0),
    source_ref TEXT NOT NULL CHECK (btrim(source_ref) <> ''),
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (holding_ref, source_ref),
    UNIQUE (holding_ref, idempotency_key),
    FOREIGN KEY (holding_ref, budget_id)
        REFERENCES erpx_tenant.finance_budgets (holding_ref, budget_id)
);

CREATE INDEX IF NOT EXISTS finance_budgets_company_idx
    ON erpx_tenant.finance_budgets (holding_ref, company_id, fiscal_year, status, updated_at DESC);
CREATE INDEX IF NOT EXISTS finance_budget_commitments_control_idx
    ON erpx_tenant.finance_budget_commitments
        (holding_ref, budget_id, account_id, period, created_at, commitment_id);

ALTER TABLE erpx_tenant.finance_budgets ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_budgets FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_budget_commitments ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_budget_commitments FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_finance_budgets ON erpx_tenant.finance_budgets;
CREATE POLICY tenant_scope_finance_budgets ON erpx_tenant.finance_budgets
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_finance_budget_commitments
    ON erpx_tenant.finance_budget_commitments;
CREATE POLICY tenant_scope_finance_budget_commitments
    ON erpx_tenant.finance_budget_commitments
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

CREATE OR REPLACE FUNCTION erpx_tenant.reject_finance_budget_commitment_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'Finance budget commitments are immutable';
END;
$$;

DROP TRIGGER IF EXISTS finance_budget_commitments_immutable
    ON erpx_tenant.finance_budget_commitments;
CREATE TRIGGER finance_budget_commitments_immutable
    BEFORE UPDATE OR DELETE ON erpx_tenant.finance_budget_commitments
    FOR EACH ROW
    EXECUTE FUNCTION erpx_tenant.reject_finance_budget_commitment_mutation();
