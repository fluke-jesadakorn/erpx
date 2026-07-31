-- Authoritative bank account, statement import, transaction, and
-- reconciliation facts for one routed holding. Provider credentials and
-- payment instruments are intentionally not represented.
CREATE SCHEMA IF NOT EXISTS erpx_tenant;

CREATE TABLE IF NOT EXISTS erpx_tenant.finance_bank_accounts (
    account_id UUID PRIMARY KEY,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    scope JSONB NOT NULL,
    currency CHAR(3) NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    account_reference TEXT NOT NULL CHECK (btrim(account_reference) <> ''),
    display_name TEXT NOT NULL CHECK (btrim(display_name) <> ''),
    version BIGINT NOT NULL CHECK (version > 0),
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (holding_ref, account_reference)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.finance_bank_statement_imports (
    import_id UUID PRIMARY KEY,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    bank_account_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    scope JSONB NOT NULL,
    statement_reference TEXT NOT NULL CHECK (btrim(statement_reference) <> ''),
    starts_on DATE NOT NULL,
    ends_on DATE NOT NULL CHECK (ends_on >= starts_on),
    opening_balance_minor BIGINT NOT NULL,
    closing_balance_minor BIGINT NOT NULL,
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    content_digest TEXT NOT NULL CHECK (length(content_digest) = 64),
    version BIGINT NOT NULL CHECK (version > 0),
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (holding_ref, idempotency_key)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.finance_bank_transactions (
    transaction_id UUID PRIMARY KEY,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    import_id UUID NOT NULL,
    bank_account_id UUID NOT NULL,
    external_transaction_id TEXT NOT NULL CHECK (btrim(external_transaction_id) <> ''),
    posted_on DATE NOT NULL,
    amount_minor BIGINT NOT NULL CHECK (amount_minor <> 0),
    currency CHAR(3) NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    description TEXT NOT NULL,
    identity_key TEXT NOT NULL CHECK (btrim(identity_key) <> ''),
    content_digest TEXT NOT NULL CHECK (length(content_digest) = 64),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (holding_ref, identity_key)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.finance_bank_reconciliations (
    reconciliation_id UUID PRIMARY KEY,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    bank_account_id UUID NOT NULL,
    statement_import_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    scope JSONB NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('draft', 'inprogress', 'exception', 'reconciled', 'closed')),
    version BIGINT NOT NULL CHECK (version > 0),
    payload JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS erpx_tenant.finance_banking_command_idempotency (
    idempotency_key TEXT PRIMARY KEY CHECK (btrim(idempotency_key) <> ''),
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    operation TEXT NOT NULL CHECK (btrim(operation) <> ''),
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    request_hash TEXT NOT NULL CHECK (length(request_hash) = 64),
    response JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS finance_bank_accounts_scope_idx
    ON erpx_tenant.finance_bank_accounts (holding_ref, company_id, account_id);
CREATE INDEX IF NOT EXISTS finance_bank_imports_scope_idx
    ON erpx_tenant.finance_bank_statement_imports
        (holding_ref, company_id, bank_account_id, ends_on DESC);
CREATE INDEX IF NOT EXISTS finance_bank_transactions_scope_idx
    ON erpx_tenant.finance_bank_transactions
        (holding_ref, bank_account_id, posted_on DESC, transaction_id);
CREATE INDEX IF NOT EXISTS finance_bank_reconciliations_scope_idx
    ON erpx_tenant.finance_bank_reconciliations
        (holding_ref, company_id, status, updated_at DESC);

ALTER TABLE erpx_tenant.finance_bank_accounts ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_bank_accounts FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_bank_statement_imports ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_bank_statement_imports FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_bank_transactions ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_bank_transactions FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_bank_reconciliations ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_bank_reconciliations FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_banking_command_idempotency ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_banking_command_idempotency FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_finance_bank_accounts
    ON erpx_tenant.finance_bank_accounts;
CREATE POLICY tenant_scope_finance_bank_accounts
    ON erpx_tenant.finance_bank_accounts
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

DROP POLICY IF EXISTS tenant_scope_finance_bank_imports
    ON erpx_tenant.finance_bank_statement_imports;
CREATE POLICY tenant_scope_finance_bank_imports
    ON erpx_tenant.finance_bank_statement_imports
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

DROP POLICY IF EXISTS tenant_scope_finance_bank_transactions
    ON erpx_tenant.finance_bank_transactions;
CREATE POLICY tenant_scope_finance_bank_transactions
    ON erpx_tenant.finance_bank_transactions
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

DROP POLICY IF EXISTS tenant_scope_finance_bank_reconciliations
    ON erpx_tenant.finance_bank_reconciliations;
CREATE POLICY tenant_scope_finance_bank_reconciliations
    ON erpx_tenant.finance_bank_reconciliations
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

DROP POLICY IF EXISTS tenant_scope_finance_banking_idempotency
    ON erpx_tenant.finance_banking_command_idempotency;
CREATE POLICY tenant_scope_finance_banking_idempotency
    ON erpx_tenant.finance_banking_command_idempotency
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
