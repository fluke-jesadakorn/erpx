CREATE SCHEMA IF NOT EXISTS erpx_control;

CREATE TABLE IF NOT EXISTS erpx_control.idempotency_keys (
    idempotency_key TEXT PRIMARY KEY,
    request_hash TEXT NOT NULL,
    command_id UUID,
    aggregate_type TEXT,
    aggregate_id UUID,
    aggregate_version BIGINT,
    event_id UUID,
    status TEXT NOT NULL CHECK (status IN ('in_progress', 'completed', 'failed')),
    response JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ
);

ALTER TABLE erpx_control.idempotency_keys
    ADD COLUMN IF NOT EXISTS aggregate_type TEXT,
    ADD COLUMN IF NOT EXISTS aggregate_id UUID,
    ADD COLUMN IF NOT EXISTS aggregate_version BIGINT,
    ADD COLUMN IF NOT EXISTS event_id UUID;

CREATE TABLE IF NOT EXISTS erpx_control.customers (
    id UUID PRIMARY KEY,
    account_code TEXT NOT NULL UNIQUE,
    trading_name TEXT NOT NULL,
    legal_name TEXT,
    country TEXT NOT NULL,
    status TEXT NOT NULL,
    billing_state TEXT NOT NULL,
    search_document TSVECTOR GENERATED ALWAYS AS (
        to_tsvector('simple', coalesce(account_code, '') || ' ' || coalesce(trading_name, '') || ' ' || coalesce(legal_name, ''))
    ) STORED,
    payload JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS customers_search_document_idx
    ON erpx_control.customers USING GIN (search_document);

CREATE TABLE IF NOT EXISTS erpx_control.support_tickets (
    id UUID PRIMARY KEY,
    customer_account_id UUID NOT NULL,
    status TEXT NOT NULL,
    severity TEXT NOT NULL,
    version BIGINT NOT NULL CHECK (version > 0),
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS support_tickets_customer_idx
    ON erpx_control.support_tickets (customer_account_id, updated_at DESC);

CREATE TABLE IF NOT EXISTS erpx_control.support_sessions (
    id UUID PRIMARY KEY,
    ticket_id UUID NOT NULL,
    customer_account_id UUID NOT NULL,
    status TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    version BIGINT NOT NULL CHECK (version > 0),
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS support_sessions_customer_idx
    ON erpx_control.support_sessions (customer_account_id, expires_at);

CREATE TABLE IF NOT EXISTS erpx_control.role_assignments (
    id UUID PRIMARY KEY,
    principal_ref TEXT NOT NULL,
    role_key TEXT NOT NULL,
    parent_assignment_id UUID,
    valid_from TIMESTAMPTZ NOT NULL,
    valid_until TIMESTAMPTZ NOT NULL,
    assignment_version INTEGER NOT NULL CHECK (assignment_version > 0),
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS role_assignments_expiry_idx
    ON erpx_control.role_assignments (valid_until, principal_ref);

CREATE TABLE IF NOT EXISTS erpx_control.audit_events (
    id UUID PRIMARY KEY,
    actor_ref TEXT NOT NULL,
    customer_account_id UUID,
    action TEXT NOT NULL,
    purpose TEXT NOT NULL,
    correlation_id UUID NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    payload JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE INDEX IF NOT EXISTS audit_events_customer_idx
    ON erpx_control.audit_events (customer_account_id, recorded_at DESC);

CREATE TABLE IF NOT EXISTS erpx_control.outbox_events (
    id UUID PRIMARY KEY,
    event_type TEXT NOT NULL,
    event_version INTEGER NOT NULL CHECK (event_version > 0),
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    tenant_scope JSONB,
    actor JSONB,
    correlation_id UUID NOT NULL,
    causation_id UUID,
    idempotency_key TEXT,
    payload JSONB NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    available_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    max_attempts INTEGER NOT NULL DEFAULT 10 CHECK (max_attempts > 0),
    claimed_by UUID,
    claimed_at TIMESTAMPTZ,
    published_at TIMESTAMPTZ,
    dead_lettered_at TIMESTAMPTZ,
    last_error TEXT,
    CONSTRAINT outbox_idempotency_key_unique UNIQUE (idempotency_key)
);

CREATE INDEX IF NOT EXISTS outbox_ready_idx
    ON erpx_control.outbox_events (available_at, occurred_at, id)
    WHERE published_at IS NULL AND dead_lettered_at IS NULL;

CREATE TABLE IF NOT EXISTS erpx_control.projection_checkpoints (
    projection_name TEXT NOT NULL,
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    last_event_id UUID,
    last_aggregate_version BIGINT NOT NULL CHECK (last_aggregate_version >= 0),
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, aggregate_type, aggregate_id)
);

CREATE INDEX IF NOT EXISTS projection_checkpoints_projection_idx
    ON erpx_control.projection_checkpoints (projection_name, updated_at);

CREATE TABLE IF NOT EXISTS erpx_control.projection_event_receipts (
    projection_name TEXT NOT NULL,
    event_id UUID NOT NULL,
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    claimed_by UUID,
    claimed_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    PRIMARY KEY (projection_name, event_id)
);

CREATE INDEX IF NOT EXISTS projection_event_receipts_claim_idx
    ON erpx_control.projection_event_receipts (projection_name, claimed_at)
    WHERE completed_at IS NULL;
