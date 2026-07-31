-- Durable bank-to-GL matching proposals and decisions.  These rows are
-- tenant-local workflow facts: posted journals remain immutable Finance
-- source records and are referenced by ID only.
CREATE SCHEMA IF NOT EXISTS erpx_tenant;

CREATE TABLE IF NOT EXISTS erpx_tenant.finance_bank_match_proposals (
    proposal_id UUID PRIMARY KEY,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    bank_account_id UUID NOT NULL,
    transaction_id UUID NOT NULL,
    scope JSONB NOT NULL,
    resolution JSONB NOT NULL,
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (holding_ref, proposal_id),
    UNIQUE (holding_ref, idempotency_key)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.finance_bank_match_decisions (
    decision_id UUID PRIMARY KEY,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    proposal_id UUID NOT NULL,
    proposal_idempotency_key TEXT NOT NULL CHECK (btrim(proposal_idempotency_key) <> ''),
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    bank_account_id UUID NOT NULL,
    transaction_id UUID NOT NULL,
    scope JSONB NOT NULL,
    resolution JSONB NOT NULL,
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (holding_ref, decision_id),
    UNIQUE (holding_ref, idempotency_key),
    UNIQUE (holding_ref, proposal_id),
    FOREIGN KEY (holding_ref, proposal_id)
        REFERENCES erpx_tenant.finance_bank_match_proposals (holding_ref, proposal_id)
);

CREATE INDEX IF NOT EXISTS finance_bank_match_proposals_scope_idx
    ON erpx_tenant.finance_bank_match_proposals
        (holding_ref, company_id, bank_account_id, transaction_id, created_at DESC);
CREATE INDEX IF NOT EXISTS finance_bank_match_decisions_scope_idx
    ON erpx_tenant.finance_bank_match_decisions
        (holding_ref, company_id, bank_account_id, transaction_id, created_at DESC);
CREATE INDEX IF NOT EXISTS finance_bank_match_decisions_proposal_idx
    ON erpx_tenant.finance_bank_match_decisions
        (holding_ref, proposal_id, created_at DESC);

ALTER TABLE erpx_tenant.finance_bank_match_proposals ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_bank_match_proposals FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_bank_match_decisions ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_bank_match_decisions FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_finance_bank_match_proposals
    ON erpx_tenant.finance_bank_match_proposals;
CREATE POLICY tenant_scope_finance_bank_match_proposals
    ON erpx_tenant.finance_bank_match_proposals
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

DROP POLICY IF EXISTS tenant_scope_finance_bank_match_decisions
    ON erpx_tenant.finance_bank_match_decisions;
CREATE POLICY tenant_scope_finance_bank_match_decisions
    ON erpx_tenant.finance_bank_match_decisions
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
