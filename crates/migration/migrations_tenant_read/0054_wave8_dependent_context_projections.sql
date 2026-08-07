-- Disposable Wave 8 projections. Every table is fenced by a projection
-- generation so rebuilds can be populated and activated without reading or
-- blocking the authoritative write database.

CREATE SCHEMA IF NOT EXISTS erpx_finance_read;

CREATE TABLE erpx_finance_read.chart_projection (
    projection_name TEXT NOT NULL DEFAULT 'finance.charts'
        CHECK (projection_name = 'finance.charts'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    chart_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    chart_code TEXT NOT NULL,
    chart_name TEXT NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    accounts JSONB NOT NULL CHECK (jsonb_typeof(accounts) = 'array'),
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, chart_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (generation, scope_key, chart_code)
);

CREATE TABLE erpx_finance_read.period_projection (
    projection_name TEXT NOT NULL DEFAULT 'finance.periods'
        CHECK (projection_name = 'finance.periods'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    period_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    period_code TEXT NOT NULL,
    starts_on DATE NOT NULL,
    ends_on DATE NOT NULL,
    period_status TEXT NOT NULL CHECK (period_status IN ('open', 'closing', 'closed')),
    close_requested_by TEXT,
    closed_by TEXT,
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, period_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (generation, scope_key, period_code)
);

CREATE TABLE erpx_finance_read.journal_projection (
    projection_name TEXT NOT NULL DEFAULT 'finance.journals'
        CHECK (projection_name = 'finance.journals'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    journal_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    chart_id UUID NOT NULL,
    period_id UUID NOT NULL,
    journal_code TEXT NOT NULL,
    entry_date DATE NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    source_kind TEXT NOT NULL,
    source JSONB NOT NULL,
    description TEXT NOT NULL,
    lines JSONB NOT NULL CHECK (jsonb_typeof(lines) = 'array'),
    debit_total_minor BIGINT NOT NULL,
    credit_total_minor BIGINT NOT NULL,
    journal_status TEXT NOT NULL CHECK (journal_status IN ('draft', 'posted')),
    posted_at TIMESTAMPTZ,
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, journal_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (generation, scope_key, journal_code),
    CHECK (debit_total_minor = credit_total_minor)
);

CREATE TABLE erpx_finance_read.subledger_projection (
    projection_name TEXT NOT NULL DEFAULT 'finance.subledger'
        CHECK (projection_name = 'finance.subledger'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    subledger_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    subledger_kind TEXT NOT NULL CHECK (subledger_kind IN ('receivable', 'payable')),
    party_id UUID NOT NULL,
    document_code TEXT NOT NULL,
    source_reference TEXT NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    original_minor BIGINT NOT NULL,
    outstanding_minor BIGINT NOT NULL,
    subledger_status TEXT NOT NULL CHECK (
        subledger_status IN ('open', 'partially_settled', 'settled')
    ),
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, subledger_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (generation, scope_key, subledger_kind, source_reference)
);

CREATE TABLE erpx_finance_read.trial_balance_projection (
    projection_name TEXT NOT NULL DEFAULT 'finance.trial_balance'
        CHECK (projection_name = 'finance.trial_balance'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    chart_id UUID NOT NULL,
    account_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    account_code TEXT NOT NULL,
    account_name TEXT NOT NULL,
    debit_minor BIGINT NOT NULL,
    credit_minor BIGINT NOT NULL,
    net_minor BIGINT NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, chart_id, account_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE
);

CREATE TABLE erpx_finance_read.bank_transaction_projection (
    projection_name TEXT NOT NULL DEFAULT 'finance.bank_transactions'
        CHECK (projection_name = 'finance.bank_transactions'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    transaction_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    journal_id UUID NOT NULL,
    import_id UUID NOT NULL,
    reference TEXT NOT NULL,
    posted_on DATE NOT NULL,
    amount_minor BIGINT NOT NULL CHECK (amount_minor <> 0),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, transaction_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (generation, import_id, transaction_id)
);

CREATE TABLE erpx_finance_read.statutory_tax_projection (
    projection_name TEXT NOT NULL DEFAULT 'finance.statutory_tax'
        CHECK (projection_name = 'finance.statutory_tax'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    tax_fact_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    journal_id UUID NOT NULL,
    reference TEXT NOT NULL,
    tax_date DATE NOT NULL,
    tax_period TEXT NOT NULL,
    tax_kind TEXT NOT NULL CHECK (tax_kind IN ('output_vat', 'input_vat', 'withholding')),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    base_minor BIGINT NOT NULL,
    tax_minor BIGINT NOT NULL,
    tax_rate_basis_points INTEGER NOT NULL,
    tax_identifier TEXT NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, tax_fact_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE
);

CREATE INDEX finance_read_chart_scope_updated
    ON erpx_finance_read.chart_projection
        (generation, scope_key, aggregate_updated_at DESC, chart_id DESC);
CREATE INDEX finance_read_period_scope_status
    ON erpx_finance_read.period_projection
        (generation, scope_key, period_status, ends_on DESC, period_id DESC);
CREATE INDEX finance_read_journal_scope_period_status
    ON erpx_finance_read.journal_projection
        (generation, scope_key, period_id, journal_status, entry_date DESC, journal_id DESC);
CREATE INDEX finance_read_subledger_scope_status
    ON erpx_finance_read.subledger_projection
        (generation, scope_key, subledger_kind, subledger_status,
         aggregate_updated_at DESC, subledger_id DESC);
CREATE INDEX finance_read_trial_balance_scope_chart
    ON erpx_finance_read.trial_balance_projection
        (generation, scope_key, chart_id, account_code, account_id);
CREATE INDEX finance_read_bank_scope_date
    ON erpx_finance_read.bank_transaction_projection
        (generation, scope_key, posted_on DESC, transaction_id DESC);
CREATE INDEX finance_read_tax_scope_period
    ON erpx_finance_read.statutory_tax_projection
        (generation, scope_key, tax_period, currency, tax_date, tax_fact_id);

CREATE SCHEMA IF NOT EXISTS erpx_legal_read;

CREATE TABLE erpx_legal_read.contract_projection (
    projection_name TEXT NOT NULL DEFAULT 'legal.contracts'
        CHECK (projection_name = 'legal.contracts'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    contract_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    reference TEXT NOT NULL,
    title TEXT NOT NULL,
    object_ref TEXT NOT NULL,
    content_sha256 TEXT NOT NULL,
    retention_until TIMESTAMPTZ NOT NULL,
    contract_status TEXT NOT NULL CHECK (contract_status IN (
        'draft', 'review', 'approved', 'active', 'expired', 'terminated', 'archived'
    )),
    parties JSONB NOT NULL CHECK (jsonb_typeof(parties) = 'array'),
    created_by TEXT NOT NULL,
    approved_by TEXT,
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, contract_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (generation, scope_key, reference)
);

CREATE TABLE erpx_legal_read.document_projection (
    projection_name TEXT NOT NULL DEFAULT 'legal.documents'
        CHECK (projection_name = 'legal.documents'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    document_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    title TEXT NOT NULL,
    versions JSONB NOT NULL CHECK (jsonb_typeof(versions) = 'array'),
    current_document_version BIGINT NOT NULL CHECK (
        current_document_version >= 0 AND current_document_version <= 1024
    ),
    archived BOOLEAN NOT NULL,
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, document_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE
);

CREATE TABLE erpx_legal_read.access_history_projection (
    projection_name TEXT NOT NULL DEFAULT 'legal.access_history'
        CHECK (projection_name = 'legal.access_history'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    access_entry_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version = 1),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    document_id UUID NOT NULL,
    document_version BIGINT NOT NULL CHECK (
        document_version > 0 AND document_version <= 1024
    ),
    actor_ref TEXT NOT NULL,
    action TEXT NOT NULL,
    purpose TEXT NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL,
    state JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, access_entry_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE
);

CREATE INDEX legal_read_contract_scope_status
    ON erpx_legal_read.contract_projection
        (generation, scope_key, contract_status, aggregate_updated_at DESC, contract_id DESC);
CREATE INDEX legal_read_document_scope_archived
    ON erpx_legal_read.document_projection
        (generation, scope_key, archived, aggregate_updated_at DESC, document_id DESC);
CREATE INDEX legal_read_access_scope_document_time
    ON erpx_legal_read.access_history_projection
        (generation, scope_key, document_id, observed_at DESC, access_entry_id DESC);

CREATE SCHEMA IF NOT EXISTS erpx_support_read;

CREATE TABLE erpx_support_read.ticket_projection (
    projection_name TEXT NOT NULL DEFAULT 'support.tickets'
        CHECK (projection_name = 'support.tickets'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    ticket_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
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
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, ticket_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (generation, scope_key, case_code)
);

CREATE TABLE erpx_support_read.ticket_customer_message_projection (
    projection_name TEXT NOT NULL DEFAULT 'support.tickets'
        CHECK (projection_name = 'support.tickets'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    ticket_id UUID NOT NULL,
    message_id UUID NOT NULL,
    author_ref TEXT NOT NULL,
    body TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, ticket_id, message_id),
    FOREIGN KEY (projection_name, generation, partition_key, ticket_id)
        REFERENCES erpx_support_read.ticket_projection
            (projection_name, generation, partition_key, ticket_id)
        ON DELETE CASCADE
);

CREATE TABLE erpx_support_read.ticket_internal_message_projection (
    projection_name TEXT NOT NULL DEFAULT 'support.tickets'
        CHECK (projection_name = 'support.tickets'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    ticket_id UUID NOT NULL,
    message_id UUID NOT NULL,
    author_ref TEXT NOT NULL,
    audience TEXT NOT NULL CHECK (audience IN ('customer_visible', 'internal')),
    body TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, ticket_id, message_id),
    FOREIGN KEY (projection_name, generation, partition_key, ticket_id)
        REFERENCES erpx_support_read.ticket_projection
            (projection_name, generation, partition_key, ticket_id)
        ON DELETE CASCADE
);

CREATE TABLE erpx_support_read.incident_projection (
    projection_name TEXT NOT NULL DEFAULT 'support.incidents'
        CHECK (projection_name = 'support.incidents'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    incident_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    incident_code TEXT NOT NULL,
    severity TEXT NOT NULL,
    commander_ref TEXT NOT NULL,
    affected_services JSONB NOT NULL,
    affected_customer_accounts JSONB NOT NULL,
    incident_status TEXT NOT NULL CHECK (incident_status IN (
        'investigating', 'identified', 'monitoring', 'resolved', 'postmortem_complete'
    )),
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, incident_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (generation, scope_key, incident_code)
);

CREATE TABLE erpx_support_read.session_projection (
    projection_name TEXT NOT NULL DEFAULT 'support.sessions'
        CHECK (projection_name = 'support.sessions'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    session_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    approved_scope JSONB NOT NULL CHECK (jsonb_typeof(approved_scope) = 'object'),
    scope_key TEXT NOT NULL,
    ticket_id UUID NOT NULL,
    customer_id UUID NOT NULL,
    customer_account_id UUID NOT NULL,
    agent_principal_id UUID NOT NULL,
    represented_principal_id UUID,
    support_mode TEXT NOT NULL CHECK (support_mode IN (
        'diagnostic_read', 'tenant_mutation', 'impersonation'
    )),
    permissions JSONB NOT NULL CHECK (
        jsonb_typeof(permissions) = 'array' AND jsonb_array_length(permissions) > 0
    ),
    module TEXT NOT NULL,
    reason TEXT NOT NULL,
    session_status TEXT NOT NULL CHECK (session_status IN ('requested', 'approved', 'revoked')),
    requested_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    approved_by TEXT,
    approved_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    revoked_reason TEXT,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, session_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    CHECK (expires_at > requested_at),
    CHECK ((support_mode = 'impersonation') = (represented_principal_id IS NOT NULL)),
    CHECK ((approved_by IS NULL) = (approved_at IS NULL)),
    CHECK ((revoked_at IS NULL) = (revoked_reason IS NULL)),
    CHECK (
        (session_status = 'requested' AND approved_at IS NULL AND revoked_at IS NULL)
        OR (session_status = 'approved' AND approved_at IS NOT NULL AND revoked_at IS NULL)
        OR (session_status = 'revoked' AND revoked_at IS NOT NULL)
    )
);

CREATE TABLE erpx_support_read.capability_projection (
    projection_name TEXT NOT NULL DEFAULT 'support.capabilities'
        CHECK (projection_name = 'support.capabilities'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    capability_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    session_id UUID NOT NULL,
    ticket_id UUID NOT NULL,
    customer_id UUID NOT NULL,
    customer_account_id UUID NOT NULL,
    agent_principal_id UUID NOT NULL,
    device_ref TEXT NOT NULL,
    support_mode TEXT NOT NULL CHECK (support_mode IN (
        'diagnostic_read', 'tenant_mutation', 'impersonation'
    )),
    permissions JSONB NOT NULL CHECK (
        jsonb_typeof(permissions) = 'array' AND jsonb_array_length(permissions) > 0
    ),
    approved_scope JSONB NOT NULL CHECK (jsonb_typeof(approved_scope) = 'object'),
    scope_key TEXT NOT NULL,
    module TEXT NOT NULL,
    issued_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    revocation_reason TEXT CHECK (revocation_reason IN (
        'customer_requested', 'agent_disabled', 'device_revoked',
        'session_revoked', 'ticket_closed', 'administrative'
    )),
    revoked_at TIMESTAMPTZ,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, capability_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    CHECK (expires_at > issued_at),
    CHECK ((revocation_reason IS NULL) = (revoked_at IS NULL))
);

CREATE INDEX support_read_ticket_scope_customer_status
    ON erpx_support_read.ticket_projection
        (generation, scope_key, customer_id, ticket_status,
         aggregate_updated_at DESC, ticket_id DESC);
CREATE INDEX support_read_incident_scope_status
    ON erpx_support_read.incident_projection
        (generation, scope_key, incident_status,
         aggregate_updated_at DESC, incident_id DESC);
CREATE INDEX support_read_session_scope_ticket_status
    ON erpx_support_read.session_projection
        (generation, scope_key, ticket_id, session_status, expires_at DESC, session_id DESC);
CREATE INDEX support_read_capability_scope_session_expiry
    ON erpx_support_read.capability_projection
        (generation, scope_key, session_id, expires_at DESC, capability_id DESC);
