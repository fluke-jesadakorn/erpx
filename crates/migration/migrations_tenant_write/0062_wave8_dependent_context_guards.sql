-- Wave 8 dependent-context guards. Migration 0081 promotes the Finance and
-- Legal roots plus typed children to current command authority. Idempotency
-- receipts, audit rows, and typed outbox facts remain atomic in erpx_cqrs_write.

CREATE SCHEMA IF NOT EXISTS erpx_finance_write;

CREATE TABLE erpx_finance_write.chart_guards (
    chart_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    chart_code TEXT NOT NULL,
    chart_name TEXT NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (updated_at >= created_at),
    UNIQUE (scope_key, chart_code)
);

CREATE TABLE erpx_finance_write.account_guards (
    chart_id UUID NOT NULL,
    account_id UUID NOT NULL,
    account_code TEXT NOT NULL,
    account_name TEXT NOT NULL,
    category TEXT NOT NULL CHECK (category IN (
        'asset', 'liability', 'equity', 'revenue', 'expense'
    )),
    normal_balance TEXT NOT NULL CHECK (normal_balance IN ('debit', 'credit')),
    parent_account_id UUID,
    account_status TEXT NOT NULL CHECK (account_status IN ('active', 'inactive')),
    PRIMARY KEY (chart_id, account_id),
    UNIQUE (chart_id, account_code),
    FOREIGN KEY (chart_id)
        REFERENCES erpx_finance_write.chart_guards (chart_id) ON DELETE CASCADE,
    FOREIGN KEY (chart_id, parent_account_id)
        REFERENCES erpx_finance_write.account_guards (chart_id, account_id)
        DEFERRABLE INITIALLY DEFERRED,
    CHECK (parent_account_id IS NULL OR parent_account_id <> account_id)
);

CREATE TABLE erpx_finance_write.period_guards (
    period_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    period_code TEXT NOT NULL,
    starts_on DATE NOT NULL,
    ends_on DATE NOT NULL,
    period_status TEXT NOT NULL CHECK (period_status IN ('open', 'closing', 'closed')),
    close_requested_by TEXT,
    closed_by TEXT,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (ends_on >= starts_on),
    CHECK (updated_at >= created_at),
    CHECK (close_requested_by IS NULL OR close_requested_by <> closed_by),
    CHECK (
        (period_status = 'open' AND close_requested_by IS NULL AND closed_by IS NULL)
        OR (period_status = 'closing' AND close_requested_by IS NOT NULL AND closed_by IS NULL)
        OR (period_status = 'closed' AND close_requested_by IS NOT NULL AND closed_by IS NOT NULL)
    ),
    UNIQUE (scope_key, period_code)
);

CREATE TABLE erpx_finance_write.journal_guards (
    journal_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    chart_id UUID NOT NULL REFERENCES erpx_finance_write.chart_guards (chart_id),
    period_id UUID NOT NULL REFERENCES erpx_finance_write.period_guards (period_id),
    journal_code TEXT NOT NULL,
    entry_date DATE NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    source_kind TEXT NOT NULL CHECK (source_kind IN (
        'sales_order', 'inventory_movement', 'payroll', 'pos_sale',
        'pos_refund', 'bank_statement', 'tax_adjustment', 'manual'
    )),
    source_reference TEXT NOT NULL,
    description TEXT NOT NULL,
    debit_total_minor BIGINT NOT NULL CHECK (debit_total_minor > 0),
    credit_total_minor BIGINT NOT NULL CHECK (credit_total_minor > 0),
    journal_status TEXT NOT NULL CHECK (journal_status IN ('draft', 'posted')),
    posted_at TIMESTAMPTZ,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (debit_total_minor = credit_total_minor),
    CHECK ((journal_status = 'posted') = (posted_at IS NOT NULL)),
    CHECK (updated_at >= created_at),
    UNIQUE (scope_key, journal_code),
    UNIQUE (journal_id, chart_id)
);

CREATE TABLE erpx_finance_write.journal_line_guards (
    journal_id UUID NOT NULL REFERENCES erpx_finance_write.journal_guards (journal_id)
        ON DELETE CASCADE,
    chart_id UUID NOT NULL,
    line_id UUID NOT NULL,
    account_id UUID NOT NULL,
    debit_minor BIGINT NOT NULL CHECK (debit_minor >= 0),
    credit_minor BIGINT NOT NULL CHECK (credit_minor >= 0),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    memo TEXT NOT NULL,
    PRIMARY KEY (journal_id, line_id),
    FOREIGN KEY (journal_id, chart_id)
        REFERENCES erpx_finance_write.journal_guards (journal_id, chart_id)
        ON DELETE CASCADE,
    FOREIGN KEY (chart_id, account_id)
        REFERENCES erpx_finance_write.account_guards (chart_id, account_id),
    CHECK ((debit_minor > 0) <> (credit_minor > 0))
);

CREATE TABLE erpx_finance_write.bank_transaction_facts (
    journal_id UUID NOT NULL REFERENCES erpx_finance_write.journal_guards (journal_id),
    import_id UUID NOT NULL,
    transaction_id UUID NOT NULL,
    reference TEXT NOT NULL,
    posted_on DATE NOT NULL,
    amount_minor BIGINT NOT NULL CHECK (amount_minor <> 0),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    PRIMARY KEY (journal_id, transaction_id),
    UNIQUE (journal_id),
    UNIQUE (import_id, transaction_id)
);

CREATE TABLE erpx_finance_write.statutory_tax_facts (
    journal_id UUID NOT NULL REFERENCES erpx_finance_write.journal_guards (journal_id),
    tax_fact_id UUID PRIMARY KEY,
    reference TEXT NOT NULL,
    tax_date DATE NOT NULL,
    tax_period TEXT NOT NULL,
    tax_kind TEXT NOT NULL CHECK (tax_kind IN ('output_vat', 'input_vat', 'withholding')),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    base_minor BIGINT NOT NULL CHECK (base_minor >= 0),
    tax_minor BIGINT NOT NULL CHECK (tax_minor >= 0),
    tax_rate_basis_points INTEGER NOT NULL CHECK (
        tax_rate_basis_points >= 0 AND tax_rate_basis_points <= 10000
    ),
    tax_identifier TEXT NOT NULL,
    UNIQUE (journal_id)
);

CREATE OR REPLACE FUNCTION erpx_finance_write.reject_source_fact_mutation()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'Finance source facts are append-only';
END
$$;
CREATE TRIGGER finance_bank_transaction_fact_immutable
    BEFORE UPDATE OR DELETE ON erpx_finance_write.bank_transaction_facts
    FOR EACH ROW EXECUTE FUNCTION erpx_finance_write.reject_source_fact_mutation();
CREATE TRIGGER finance_statutory_tax_fact_immutable
    BEFORE UPDATE OR DELETE ON erpx_finance_write.statutory_tax_facts
    FOR EACH ROW EXECUTE FUNCTION erpx_finance_write.reject_source_fact_mutation();

CREATE TABLE erpx_finance_write.subledger_guards (
    subledger_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    subledger_kind TEXT NOT NULL CHECK (subledger_kind IN ('receivable', 'payable')),
    party_id UUID NOT NULL,
    document_code TEXT NOT NULL,
    source_reference TEXT NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    original_minor BIGINT NOT NULL CHECK (original_minor > 0),
    outstanding_minor BIGINT NOT NULL CHECK (
        outstanding_minor >= 0 AND outstanding_minor <= original_minor
    ),
    subledger_status TEXT NOT NULL CHECK (subledger_status IN ('open', 'partially_settled', 'settled')),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (updated_at >= created_at),
    CHECK (
        (subledger_status = 'open' AND outstanding_minor = original_minor)
        OR (subledger_status = 'partially_settled'
            AND outstanding_minor > 0 AND outstanding_minor < original_minor)
        OR (subledger_status = 'settled' AND outstanding_minor = 0)
    ),
    UNIQUE (scope_key, subledger_kind, source_reference)
);

CREATE INDEX finance_chart_scope_updated
    ON erpx_finance_write.chart_guards (scope_key, updated_at DESC, chart_id DESC);
CREATE INDEX finance_period_scope_status_dates
    ON erpx_finance_write.period_guards
        (scope_key, period_status, ends_on DESC, period_id DESC);
CREATE INDEX finance_journal_scope_period_status
    ON erpx_finance_write.journal_guards
        (scope_key, period_id, journal_status, entry_date DESC, journal_id DESC);
CREATE INDEX finance_subledger_scope_status
    ON erpx_finance_write.subledger_guards
        (scope_key, subledger_kind, subledger_status, updated_at DESC, subledger_id DESC);

CREATE SCHEMA IF NOT EXISTS erpx_legal_write;

CREATE TABLE erpx_legal_write.contract_guards (
    contract_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    reference TEXT NOT NULL CHECK (octet_length(reference) BETWEEN 1 AND 255),
    title TEXT NOT NULL CHECK (octet_length(title) BETWEEN 1 AND 255),
    object_ref TEXT NOT NULL CHECK (octet_length(object_ref) BETWEEN 1 AND 255),
    content_sha256 TEXT NOT NULL CHECK (content_sha256 ~ '^(sha256:)?[0-9A-Fa-f]{64}$'),
    retention_until TIMESTAMPTZ NOT NULL,
    contract_status TEXT NOT NULL CHECK (contract_status IN (
        'draft', 'review', 'approved', 'active', 'expired', 'terminated', 'archived'
    )),
    created_by TEXT NOT NULL CHECK (octet_length(created_by) BETWEEN 1 AND 255),
    approved_by TEXT CHECK (approved_by IS NULL OR octet_length(approved_by) BETWEEN 1 AND 255),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (updated_at >= created_at),
    CHECK (retention_until >= created_at),
    CHECK (retention_until <= created_at + INTERVAL '36525 days'),
    CHECK (approved_by IS NULL OR approved_by <> created_by),
    CHECK (
        contract_status NOT IN ('approved', 'active', 'expired', 'terminated', 'archived')
        OR approved_by IS NOT NULL
    ),
    UNIQUE (scope_key, reference)
);

CREATE TABLE erpx_legal_write.contract_party_guards (
    contract_id UUID NOT NULL REFERENCES erpx_legal_write.contract_guards (contract_id)
        ON DELETE CASCADE,
    party_id UUID NOT NULL,
    role TEXT NOT NULL CHECK (octet_length(role) BETWEEN 1 AND 255),
    PRIMARY KEY (contract_id, party_id)
);

CREATE TABLE erpx_legal_write.document_guards (
    document_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    title TEXT NOT NULL CHECK (octet_length(title) BETWEEN 1 AND 255),
    current_document_version BIGINT NOT NULL CHECK (
        current_document_version >= 0 AND current_document_version <= 1024
    ),
    archived BOOLEAN NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (updated_at >= created_at),
    CHECK (NOT archived OR current_document_version > 0)
);

CREATE TABLE erpx_legal_write.document_version_guards (
    document_id UUID NOT NULL REFERENCES erpx_legal_write.document_guards (document_id)
        ON DELETE CASCADE,
    document_version BIGINT NOT NULL CHECK (
        document_version > 0 AND document_version <= 1024
    ),
    object_ref TEXT NOT NULL CHECK (octet_length(object_ref) BETWEEN 1 AND 255),
    content_sha256 TEXT NOT NULL CHECK (content_sha256 ~ '^(sha256:)?[0-9A-Fa-f]{64}$'),
    created_at TIMESTAMPTZ NOT NULL,
    created_by TEXT NOT NULL CHECK (octet_length(created_by) BETWEEN 1 AND 255),
    PRIMARY KEY (document_id, document_version)
);

CREATE TABLE erpx_legal_write.access_entry_guards (
    access_entry_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    document_id UUID NOT NULL,
    document_version BIGINT NOT NULL,
    actor_ref TEXT NOT NULL CHECK (octet_length(actor_ref) BETWEEN 1 AND 255),
    action TEXT NOT NULL CHECK (octet_length(action) BETWEEN 1 AND 255),
    purpose TEXT NOT NULL CHECK (octet_length(purpose) BETWEEN 1 AND 512),
    observed_at TIMESTAMPTZ NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version = 1),
    FOREIGN KEY (document_id, document_version)
        REFERENCES erpx_legal_write.document_version_guards (document_id, document_version)
);

CREATE OR REPLACE FUNCTION erpx_legal_write.reject_append_only_mutation()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'Legal evidence is append-only';
END
$$;
CREATE TRIGGER legal_document_version_immutable
    BEFORE UPDATE OR DELETE ON erpx_legal_write.document_version_guards
    FOR EACH ROW EXECUTE FUNCTION erpx_legal_write.reject_append_only_mutation();
CREATE TRIGGER legal_access_entry_immutable
    BEFORE UPDATE OR DELETE ON erpx_legal_write.access_entry_guards
    FOR EACH ROW EXECUTE FUNCTION erpx_legal_write.reject_append_only_mutation();

CREATE INDEX legal_contract_scope_status
    ON erpx_legal_write.contract_guards
        (scope_key, contract_status, updated_at DESC, contract_id DESC);
CREATE INDEX legal_document_scope_archived
    ON erpx_legal_write.document_guards
        (scope_key, archived, updated_at DESC, document_id DESC);
CREATE INDEX legal_access_scope_document_time
    ON erpx_legal_write.access_entry_guards
        (scope_key, document_id, observed_at DESC, access_entry_id DESC);

CREATE SCHEMA IF NOT EXISTS erpx_support_write;

CREATE TABLE erpx_support_write.ticket_guards (
    ticket_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    customer_id UUID NOT NULL,
    customer_account_id UUID NOT NULL,
    case_code TEXT NOT NULL,
    category TEXT NOT NULL,
    severity TEXT NOT NULL,
    description TEXT NOT NULL,
    ticket_status TEXT NOT NULL CHECK (ticket_status IN (
        'new', 'triaged', 'waiting_for_customer', 'access_requested',
        'approved', 'in_progress', 'resolved', 'customer_confirmed', 'closed'
    )),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (updated_at >= created_at),
    UNIQUE (scope_key, case_code)
);

CREATE TABLE erpx_support_write.ticket_message_guards (
    ticket_id UUID NOT NULL,
    message_id UUID NOT NULL,
    author_ref TEXT NOT NULL,
    audience TEXT NOT NULL CHECK (audience IN ('customer_visible', 'internal')),
    body TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (ticket_id, message_id),
    FOREIGN KEY (ticket_id)
        REFERENCES erpx_support_write.ticket_guards (ticket_id) ON DELETE CASCADE
);

CREATE TABLE erpx_support_write.incident_guards (
    incident_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    incident_code TEXT NOT NULL,
    severity TEXT NOT NULL,
    commander_ref TEXT NOT NULL,
    affected_services JSONB NOT NULL,
    affected_customer_accounts JSONB NOT NULL,
    timeline JSONB NOT NULL,
    broadcasts JSONB NOT NULL,
    incident_status TEXT NOT NULL CHECK (incident_status IN (
        'investigating', 'identified', 'monitoring', 'resolved', 'postmortem_complete'
    )),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (jsonb_typeof(affected_services) = 'array' AND jsonb_array_length(affected_services) > 0),
    CHECK (jsonb_typeof(affected_customer_accounts) = 'array'),
    CHECK (jsonb_typeof(timeline) = 'array'),
    CHECK (jsonb_typeof(broadcasts) = 'array'),
    CHECK (updated_at >= created_at),
    UNIQUE (scope_key, incident_code)
);

CREATE TABLE erpx_support_write.session_guards (
    session_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    ticket_id UUID NOT NULL,
    customer_id UUID NOT NULL,
    customer_account_id UUID NOT NULL,
    agent_principal_id UUID NOT NULL,
    represented_principal_id UUID,
    support_mode TEXT NOT NULL CHECK (support_mode IN (
        'diagnostic_read', 'tenant_mutation', 'impersonation'
    )),
    permissions JSONB NOT NULL,
    module TEXT NOT NULL,
    reason TEXT NOT NULL,
    session_status TEXT NOT NULL CHECK (session_status IN ('requested', 'approved', 'revoked')),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    requested_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    approved_by TEXT,
    approved_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    revoked_reason TEXT,
    CHECK (jsonb_typeof(permissions) = 'array' AND jsonb_array_length(permissions) > 0),
    CHECK (expires_at > requested_at),
    CHECK ((support_mode = 'impersonation') = (represented_principal_id IS NOT NULL)),
    CHECK ((approved_by IS NULL) = (approved_at IS NULL)),
    CHECK (approved_by IS NULL OR approved_by <> agent_principal_id::TEXT),
    CHECK ((revoked_at IS NULL) = (revoked_reason IS NULL)),
    CHECK (
        (session_status = 'requested'
            AND approved_at IS NULL AND revoked_at IS NULL)
        OR (session_status = 'approved'
            AND approved_at IS NOT NULL AND revoked_at IS NULL)
        OR (session_status = 'revoked' AND revoked_at IS NOT NULL)
    ),
    FOREIGN KEY (ticket_id) REFERENCES erpx_support_write.ticket_guards (ticket_id),
    UNIQUE (
        session_id, ticket_id, customer_id, customer_account_id,
        agent_principal_id, scope_key, support_mode, module
    )
);

CREATE TABLE erpx_support_write.capability_guards (
    capability_id UUID PRIMARY KEY,
    session_id UUID NOT NULL,
    ticket_id UUID NOT NULL,
    customer_id UUID NOT NULL,
    customer_account_id UUID NOT NULL,
    agent_principal_id UUID NOT NULL,
    device_ref TEXT NOT NULL,
    support_mode TEXT NOT NULL CHECK (support_mode IN (
        'diagnostic_read', 'tenant_mutation', 'impersonation'
    )),
    permissions JSONB NOT NULL,
    approved_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    module TEXT NOT NULL,
    secret_hash_sha256 BYTEA NOT NULL CHECK (octet_length(secret_hash_sha256) = 32),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    issued_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    revocation_reason TEXT CHECK (revocation_reason IN (
        'customer_requested', 'agent_disabled', 'device_revoked',
        'session_revoked', 'ticket_closed', 'administrative'
    )),
    revoked_at TIMESTAMPTZ,
    CHECK (expires_at > issued_at),
    CHECK (jsonb_typeof(permissions) = 'array' AND jsonb_array_length(permissions) > 0),
    CHECK (jsonb_typeof(approved_scope) = 'object'),
    CHECK ((revocation_reason IS NULL) = (revoked_at IS NULL)),
    FOREIGN KEY (ticket_id) REFERENCES erpx_support_write.ticket_guards (ticket_id),
    FOREIGN KEY (
        session_id, ticket_id, customer_id, customer_account_id,
        agent_principal_id, scope_key, support_mode, module
    ) REFERENCES erpx_support_write.session_guards (
        session_id, ticket_id, customer_id, customer_account_id,
        agent_principal_id, scope_key, support_mode, module
    )
);

CREATE INDEX support_ticket_scope_customer_status
    ON erpx_support_write.ticket_guards
        (scope_key, customer_id, ticket_status, updated_at DESC, ticket_id DESC);
CREATE INDEX support_incident_scope_status
    ON erpx_support_write.incident_guards
        (scope_key, incident_status, updated_at DESC, incident_id DESC);
CREATE INDEX support_session_scope_ticket_status
    ON erpx_support_write.session_guards
        (scope_key, ticket_id, session_status, expires_at DESC, session_id DESC);
CREATE INDEX support_capability_scope_session_expiry
    ON erpx_support_write.capability_guards
        (scope_key, session_id, expires_at DESC, capability_id DESC);
