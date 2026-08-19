
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
    active_scope JSONB,
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


ALTER TABLE erpx_control.idempotency_keys
    ADD COLUMN IF NOT EXISTS aggregate_type TEXT,
    ADD COLUMN IF NOT EXISTS aggregate_id UUID,
    ADD COLUMN IF NOT EXISTS aggregate_version BIGINT,
    ADD COLUMN IF NOT EXISTS event_id UUID;

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

CREATE TABLE IF NOT EXISTS erpx_control.holding_registry (
    holding_id TEXT PRIMARY KEY,
    data_profile_key TEXT NOT NULL UNIQUE,
    write_database_key TEXT NOT NULL,
    read_database_key TEXT NOT NULL,
    storage_namespace TEXT NOT NULL,
    credential_key_ref TEXT NOT NULL,
    backup_chain_ref TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('provisioning', 'active', 'suspended', 'recovery')),
    registry_version BIGINT NOT NULL CHECK (registry_version > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS holding_registry_state_idx
    ON erpx_control.holding_registry (state, updated_at);


CREATE TABLE IF NOT EXISTS erpx_control.global_read_audits (
    id UUID PRIMARY KEY,
    actor_ref TEXT NOT NULL,
    session_ref TEXT,
    customer_id UUID NOT NULL,
    module TEXT NOT NULL,
    scope JSONB NOT NULL,
    purpose TEXT NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    correlation_id UUID NOT NULL,
    payload JSONB NOT NULL
);

CREATE INDEX IF NOT EXISTS global_read_audits_customer_idx
    ON erpx_control.global_read_audits (customer_id, occurred_at DESC);

CREATE INDEX IF NOT EXISTS global_read_audits_actor_idx
    ON erpx_control.global_read_audits (actor_ref, occurred_at DESC);


CREATE TABLE IF NOT EXISTS erpx_control.commercial_catalog_revisions (
    id UUID PRIMARY KEY,
    revision_number INTEGER NOT NULL UNIQUE CHECK (revision_number > 0),
    published_at TIMESTAMPTZ NOT NULL,
    published_by TEXT NOT NULL,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS erpx_control.grandfathered_price_migrations (
    id UUID PRIMARY KEY,
    customer_id UUID NOT NULL,
    subscription_id UUID NOT NULL,
    item_key TEXT NOT NULL,
    from_revision INTEGER NOT NULL CHECK (from_revision > 0),
    to_revision INTEGER NOT NULL CHECK (to_revision > from_revision),
    effective_at TIMESTAMPTZ NOT NULL,
    payload JSONB NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (customer_id, subscription_id, item_key, from_revision, to_revision)
);

CREATE TABLE IF NOT EXISTS erpx_control.billing_lifecycles (
    customer_id UUID NOT NULL,
    subscription_id UUID NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('trialing', 'active', 'past_due', 'grace', 'read_only', 'canceled')),
    grace_started_at TIMESTAMPTZ,
    grace_until TIMESTAMPTZ,
    last_transition_at TIMESTAMPTZ NOT NULL,
    version BIGINT NOT NULL CHECK (version > 0),
    payload JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (customer_id, subscription_id)
);

CREATE TABLE IF NOT EXISTS erpx_control.entitlement_allocation_books (
    customer_id UUID NOT NULL,
    module_key TEXT NOT NULL,
    active BOOLEAN NOT NULL DEFAULT FALSE,
    active_until TIMESTAMPTZ,
    version BIGINT NOT NULL CHECK (version > 0),
    payload JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (customer_id, module_key)
);

CREATE TABLE IF NOT EXISTS erpx_control.stripe_event_inbox (
    event_id TEXT PRIMARY KEY,
    event_type TEXT NOT NULL,
    stripe_created_at TIMESTAMPTZ,
    received_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    payload JSONB NOT NULL,
    raw_body BYTEA NOT NULL,
    status TEXT NOT NULL DEFAULT 'received'
        CHECK (status IN ('received', 'processing', 'processed', 'retryable', 'dead_letter')),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    available_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    claimed_by UUID,
    claimed_at TIMESTAMPTZ,
    claim_expires_at TIMESTAMPTZ,
    processed_at TIMESTAMPTZ,
    dead_lettered_at TIMESTAMPTZ,
    CHECK (
        status <> 'processing'
        OR (claimed_by IS NOT NULL AND claimed_at IS NOT NULL AND claim_expires_at IS NOT NULL)
    ),
    last_error TEXT
);

CREATE INDEX IF NOT EXISTS stripe_event_inbox_ready_idx
    ON erpx_control.stripe_event_inbox (available_at, received_at, event_id)
    WHERE status IN ('received', 'retryable');

CREATE INDEX IF NOT EXISTS stripe_event_inbox_claim_expiry_idx
    ON erpx_control.stripe_event_inbox (claim_expires_at, received_at, event_id)
    WHERE status = 'processing';

CREATE INDEX IF NOT EXISTS billing_lifecycles_state_idx
    ON erpx_control.billing_lifecycles (state, updated_at);


CREATE TABLE IF NOT EXISTS erpx_control.support_command_idempotency (
    idempotency_key TEXT PRIMARY KEY,
    request_hash TEXT NOT NULL,
    command_type TEXT NOT NULL CHECK (
        command_type IN (
            'ticket_insert', 'ticket_update',
            'access_request_insert', 'access_request_approve',
            'capability_issue', 'capability_revoke',
            'incident_insert', 'incident_update'
        )
    ),
    resource_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'support_tickets_status_check'
          AND conrelid = 'erpx_control.support_tickets'::regclass
    ) THEN
        ALTER TABLE erpx_control.support_tickets
            ADD CONSTRAINT support_tickets_status_check CHECK (
                status IN (
                    'new', 'triaged', 'waiting_for_customer', 'access_requested',
                    'approved', 'in_progress', 'resolved', 'customer_confirmed', 'closed'
                )
            ) NOT VALID;
    END IF;
END $$;

CREATE TABLE IF NOT EXISTS erpx_control.support_access_requests (
    id UUID PRIMARY KEY,
    ticket_id UUID NOT NULL REFERENCES erpx_control.support_tickets(id) ON DELETE RESTRICT,
    customer_account_id UUID NOT NULL,
    agent_ref TEXT NOT NULL CHECK (btrim(agent_ref) <> ''),
    represented_holding_user TEXT,
    mode TEXT NOT NULL CHECK (mode IN ('diagnostic_read', 'holding_mutation', 'impersonation')),
    permissions JSONB NOT NULL CHECK (jsonb_typeof(permissions) = 'array'),
    scope TEXT NOT NULL CHECK (btrim(scope) <> ''),
    reason TEXT NOT NULL CHECK (btrim(reason) <> ''),
    requested_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('requested', 'approved', 'revoked', 'expired')),
    approved_by TEXT,
    approved_at TIMESTAMPTZ,
    approval_ref TEXT,
    revoked_at TIMESTAMPTZ,
    version BIGINT NOT NULL CHECK (version > 0),
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (expires_at > requested_at),
    CHECK (
        (mode = 'diagnostic_read' AND expires_at <= requested_at + interval '8 hours')
        OR (mode IN ('holding_mutation', 'impersonation')
            AND expires_at <= requested_at + interval '2 hours')
    ),
    CHECK ((mode = 'impersonation') = (represented_holding_user IS NOT NULL)),
    CHECK ((approved_at IS NULL AND approved_by IS NULL AND approval_ref IS NULL)
        OR (approved_at IS NOT NULL AND approved_by IS NOT NULL AND approval_ref IS NOT NULL)),
    CHECK (
        (approved_at IS NULL AND status = 'requested')
        OR (approved_at IS NOT NULL AND status IN ('approved', 'revoked', 'expired'))
    ),
    UNIQUE (approval_ref)
);

CREATE INDEX IF NOT EXISTS support_access_requests_ticket_idx
    ON erpx_control.support_access_requests (ticket_id, requested_at DESC);

CREATE INDEX IF NOT EXISTS support_access_requests_expiry_idx
    ON erpx_control.support_access_requests (expires_at, status);

CREATE TABLE IF NOT EXISTS erpx_control.support_capabilities (
    id UUID PRIMARY KEY,
    access_request_id UUID NOT NULL REFERENCES erpx_control.support_access_requests(id) ON DELETE RESTRICT,
    customer_account_id UUID NOT NULL,
    agent_ref TEXT NOT NULL CHECK (btrim(agent_ref) <> ''),
    device_ref TEXT NOT NULL CHECK (btrim(device_ref) <> ''),
    ticket_id UUID NOT NULL REFERENCES erpx_control.support_tickets(id) ON DELETE RESTRICT,
    mode TEXT NOT NULL CHECK (mode IN ('diagnostic_read', 'holding_mutation', 'impersonation')),
    permissions JSONB NOT NULL CHECK (jsonb_typeof(permissions) = 'array'),
    scope TEXT NOT NULL CHECK (btrim(scope) <> ''),
    module TEXT NOT NULL CHECK (btrim(module) <> ''),
    issued_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    secret_hash BYTEA NOT NULL CHECK (octet_length(secret_hash) = 32),
    reissued_from UUID REFERENCES erpx_control.support_capabilities(id) ON DELETE RESTRICT,
    status TEXT NOT NULL CHECK (status IN ('active', 'revoked', 'expired')),
    revoked_at TIMESTAMPTZ,
    revocation_reason TEXT,
    version BIGINT NOT NULL CHECK (version > 0),
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (expires_at > issued_at),
    CHECK ((revoked_at IS NULL AND revocation_reason IS NULL)
        OR (revoked_at IS NOT NULL AND revocation_reason IS NOT NULL)),
    CHECK (
        (status IN ('active', 'expired') AND revoked_at IS NULL AND revocation_reason IS NULL)
        OR (status = 'revoked' AND revoked_at IS NOT NULL AND revocation_reason IS NOT NULL)
    ),
    UNIQUE (reissued_from)
);

CREATE INDEX IF NOT EXISTS support_capabilities_request_idx
    ON erpx_control.support_capabilities (access_request_id, expires_at);

CREATE INDEX IF NOT EXISTS support_capabilities_active_idx
    ON erpx_control.support_capabilities (status, expires_at);

CREATE TABLE IF NOT EXISTS erpx_control.incidents (
    id UUID PRIMARY KEY,
    status TEXT NOT NULL CHECK (
        status IN ('investigating', 'identified', 'monitoring', 'resolved', 'postmortem_complete')
    ),
    version BIGINT NOT NULL CHECK (version > 0),
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS incidents_status_idx
    ON erpx_control.incidents (status, updated_at DESC);

CREATE OR REPLACE FUNCTION erpx_control.guard_support_access_request_binding()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.ticket_id IS DISTINCT FROM NEW.ticket_id
       OR OLD.customer_account_id IS DISTINCT FROM NEW.customer_account_id
       OR OLD.agent_ref IS DISTINCT FROM NEW.agent_ref
       OR OLD.represented_holding_user IS DISTINCT FROM NEW.represented_holding_user
       OR OLD.mode IS DISTINCT FROM NEW.mode
       OR OLD.permissions IS DISTINCT FROM NEW.permissions
       OR OLD.scope IS DISTINCT FROM NEW.scope
       OR OLD.reason IS DISTINCT FROM NEW.reason
       OR OLD.requested_at IS DISTINCT FROM NEW.requested_at
       OR OLD.expires_at IS DISTINCT FROM NEW.expires_at
       OR OLD.payload->'ticket_id' IS DISTINCT FROM NEW.payload->'ticket_id'
       OR OLD.payload->'customer_account_id' IS DISTINCT FROM NEW.payload->'customer_account_id'
       OR OLD.payload->'agent_ref' IS DISTINCT FROM NEW.payload->'agent_ref'
       OR OLD.payload->'represented_holding_user' IS DISTINCT FROM NEW.payload->'represented_holding_user'
       OR OLD.payload->'mode' IS DISTINCT FROM NEW.payload->'mode'
       OR OLD.payload->'permissions' IS DISTINCT FROM NEW.payload->'permissions'
       OR OLD.payload->'scope' IS DISTINCT FROM NEW.payload->'scope'
       OR OLD.payload->'reason' IS DISTINCT FROM NEW.payload->'reason'
       OR OLD.payload->'requested_at' IS DISTINCT FROM NEW.payload->'requested_at'
       OR OLD.payload->'expires_at' IS DISTINCT FROM NEW.payload->'expires_at'
    THEN
        RAISE EXCEPTION 'support access request approval binding is immutable';
    END IF;

    IF OLD.approved_at IS NOT NULL
       AND (OLD.approved_by IS DISTINCT FROM NEW.approved_by
            OR OLD.approved_at IS DISTINCT FROM NEW.approved_at
            OR OLD.approval_ref IS DISTINCT FROM NEW.approval_ref)
    THEN
        RAISE EXCEPTION 'support access request approval decision is immutable';
    END IF;

    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS support_access_request_binding_guard
    ON erpx_control.support_access_requests;
CREATE TRIGGER support_access_request_binding_guard
BEFORE UPDATE ON erpx_control.support_access_requests
FOR EACH ROW EXECUTE FUNCTION erpx_control.guard_support_access_request_binding();

CREATE OR REPLACE FUNCTION erpx_control.guard_support_capability_binding()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    request_row erpx_control.support_access_requests%ROWTYPE;
BEGIN
    SELECT * INTO request_row
    FROM erpx_control.support_access_requests
    WHERE id = NEW.access_request_id;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'support capability requires an existing access request';
    END IF;

    IF request_row.approved_at IS NULL OR request_row.status <> 'approved' THEN
        RAISE EXCEPTION 'support capability requires an approved access request';
    END IF;

    IF NEW.ticket_id IS DISTINCT FROM request_row.ticket_id
       OR NEW.customer_account_id IS DISTINCT FROM request_row.customer_account_id
       OR NEW.agent_ref IS DISTINCT FROM request_row.agent_ref
       OR NEW.mode IS DISTINCT FROM request_row.mode
       OR NEW.permissions IS DISTINCT FROM request_row.permissions
       OR NEW.scope IS DISTINCT FROM request_row.scope
       OR NEW.issued_at < request_row.approved_at
       OR NEW.expires_at > request_row.expires_at
    THEN
        RAISE EXCEPTION 'support capability binding exceeds customer approval';
    END IF;

    IF TG_OP = 'UPDATE' AND (
        OLD.access_request_id IS DISTINCT FROM NEW.access_request_id
        OR OLD.customer_account_id IS DISTINCT FROM NEW.customer_account_id
        OR OLD.agent_ref IS DISTINCT FROM NEW.agent_ref
        OR OLD.device_ref IS DISTINCT FROM NEW.device_ref
        OR OLD.ticket_id IS DISTINCT FROM NEW.ticket_id
        OR OLD.mode IS DISTINCT FROM NEW.mode
        OR OLD.permissions IS DISTINCT FROM NEW.permissions
        OR OLD.scope IS DISTINCT FROM NEW.scope
        OR OLD.module IS DISTINCT FROM NEW.module
        OR OLD.issued_at IS DISTINCT FROM NEW.issued_at
        OR OLD.expires_at IS DISTINCT FROM NEW.expires_at
        OR OLD.secret_hash IS DISTINCT FROM NEW.secret_hash
        OR OLD.reissued_from IS DISTINCT FROM NEW.reissued_from
        OR OLD.payload IS DISTINCT FROM NEW.payload
    ) THEN
        RAISE EXCEPTION 'support capability approval binding is immutable';
    END IF;

    IF TG_OP = 'UPDATE' AND OLD.revoked_at IS NOT NULL
       AND (NEW.revoked_at IS NULL OR NEW.status <> 'revoked')
    THEN
        RAISE EXCEPTION 'support capability revocation is immutable';
    END IF;

    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS support_capability_binding_guard
    ON erpx_control.support_capabilities;
CREATE TRIGGER support_capability_binding_guard
BEFORE INSERT OR UPDATE ON erpx_control.support_capabilities
FOR EACH ROW EXECUTE FUNCTION erpx_control.guard_support_capability_binding();


CREATE TABLE IF NOT EXISTS erpx_control.sod_decisions (
    principal_ref TEXT NOT NULL,
    module_key TEXT NOT NULL,
    scope JSONB NOT NULL,
    passed BOOLEAN NOT NULL,
    reviewer_ref TEXT NOT NULL,
    reason TEXT NOT NULL,
    valid_from TIMESTAMPTZ NOT NULL,
    valid_until TIMESTAMPTZ NOT NULL,
    decision_version BIGINT NOT NULL CHECK (decision_version > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (btrim(principal_ref) <> ''),
    CHECK (btrim(module_key) <> ''),
    CHECK (btrim(reviewer_ref) <> ''),
    CHECK (btrim(reason) <> ''),
    CHECK (valid_until > valid_from),
    PRIMARY KEY (principal_ref, module_key, scope)
);

CREATE INDEX IF NOT EXISTS sod_decisions_expiry_idx
    ON erpx_control.sod_decisions (valid_until, principal_ref, module_key);


CREATE TABLE IF NOT EXISTS erpx_control.holding_provisioning (
    holding_id TEXT PRIMARY KEY REFERENCES erpx_control.holding_registry(holding_id) ON DELETE RESTRICT,
    state TEXT NOT NULL CHECK (state IN ('provisioning', 'active', 'suspended', 'recovery', 'failed')),
    write_database_key TEXT NOT NULL,
    read_database_key TEXT NOT NULL,
    storage_namespace TEXT NOT NULL,
    version BIGINT NOT NULL CHECK (version > 0),
    last_error TEXT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS erpx_control.holding_database_health (
    holding_id TEXT NOT NULL REFERENCES erpx_control.holding_registry(holding_id) ON DELETE RESTRICT,
    side TEXT NOT NULL CHECK (side IN ('write', 'read')),
    database_key TEXT NOT NULL,
    schema_version TEXT NOT NULL,
    reachable BOOLEAN NOT NULL,
    checked_at TIMESTAMPTZ NOT NULL,
    details JSONB NOT NULL DEFAULT '{}'::jsonb,
    PRIMARY KEY (holding_id, side)
);

CREATE TABLE IF NOT EXISTS erpx_control.holding_storage_health (
    holding_id TEXT PRIMARY KEY REFERENCES erpx_control.holding_registry(holding_id) ON DELETE RESTRICT,
    storage_namespace TEXT NOT NULL,
    reachable BOOLEAN NOT NULL,
    checked_at TIMESTAMPTZ NOT NULL,
    details JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE TABLE IF NOT EXISTS erpx_control.holding_backup_runs (
    id UUID PRIMARY KEY,
    holding_id TEXT NOT NULL REFERENCES erpx_control.holding_registry(holding_id) ON DELETE RESTRICT,
    kind TEXT NOT NULL CHECK (kind IN ('full', 'pitr', 'restore_test')),
    status TEXT NOT NULL CHECK (status IN ('started', 'completed', 'failed')),
    backup_ref TEXT NOT NULL,
    restore_target_ref TEXT,
    started_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,
    verified BOOLEAN NOT NULL DEFAULT FALSE,
    error TEXT,
    CHECK (btrim(backup_ref) <> ''),
    CHECK ((kind <> 'restore_test') OR restore_target_ref IS NOT NULL)
);

CREATE INDEX IF NOT EXISTS holding_backup_runs_history_idx
    ON erpx_control.holding_backup_runs (holding_id, started_at DESC);

CREATE TABLE IF NOT EXISTS erpx_control.holding_projection_switches (
    holding_id TEXT PRIMARY KEY REFERENCES erpx_control.holding_registry(holding_id) ON DELETE RESTRICT,
    current_read_database_key TEXT NOT NULL,
    candidate_read_database_key TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('rebuilding', 'ready', 'activated', 'failed')),
    source_version BIGINT NOT NULL CHECK (source_version >= 0),
    version BIGINT NOT NULL CHECK (version > 0),
    last_error TEXT,
    activated_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (current_read_database_key <> candidate_read_database_key)
);


-- Authoritative control-plane bindings between customer accounts and the
-- organization scopes that may address their holding data. A NULL company or
-- branch is a deliberately explicit wildcard at that hierarchy level.
CREATE TABLE IF NOT EXISTS erpx_control.customer_scope_bindings (
    customer_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID,
    branch_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (branch_id IS NULL OR company_id IS NOT NULL),
    UNIQUE (customer_id, holding_id, company_id, branch_id)
);

CREATE INDEX IF NOT EXISTS customer_scope_bindings_lookup_idx
    ON erpx_control.customer_scope_bindings
        (customer_id, holding_id, company_id, branch_id);

CREATE UNIQUE INDEX IF NOT EXISTS customer_scope_bindings_null_safe_uq
    ON erpx_control.customer_scope_bindings
        (customer_id, holding_id, coalesce(company_id, '00000000-0000-0000-0000-000000000000'::uuid),
         coalesce(branch_id, '00000000-0000-0000-0000-000000000000'::uuid));


ALTER TABLE erpx_control.stripe_event_inbox
    ADD COLUMN IF NOT EXISTS raw_body_sha256 BYTEA,
    ADD COLUMN IF NOT EXISTS correlation_id UUID;

ALTER TABLE erpx_control.stripe_event_inbox
    DROP CONSTRAINT IF EXISTS stripe_event_inbox_raw_body_sha256_length;

ALTER TABLE erpx_control.stripe_event_inbox
    ADD CONSTRAINT stripe_event_inbox_raw_body_sha256_length
    CHECK (raw_body_sha256 IS NULL OR octet_length(raw_body_sha256) = 32);

CREATE INDEX IF NOT EXISTS stripe_event_inbox_correlation_idx
    ON erpx_control.stripe_event_inbox (correlation_id, received_at);


-- Authoritative binding for every signed object key. A presigned upload first
-- creates this row; downloads require the existing row. The namespace/key
-- uniqueness prevents an object from being rebound to another customer or
-- organization scope.
CREATE TABLE IF NOT EXISTS erpx_control.object_access_bindings (
    id UUID PRIMARY KEY,
    holding_id TEXT NOT NULL,
    storage_namespace TEXT NOT NULL,
    object_key TEXT NOT NULL,
    customer_id UUID NOT NULL,
    company_id UUID,
    branch_id UUID,
    created_by TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('active', 'revoked')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (branch_id IS NULL OR company_id IS NOT NULL),
    UNIQUE (storage_namespace, object_key)
);

CREATE INDEX IF NOT EXISTS object_access_bindings_customer_scope_idx
    ON erpx_control.object_access_bindings
        (customer_id, holding_id, company_id, branch_id, status);


-- Durable worker liveness evidence used by the server readiness boundary.
-- A missing row is not healthy; the server applies a bounded freshness window.
CREATE TABLE IF NOT EXISTS erpx_control.worker_heartbeats (
    worker_id UUID PRIMARY KEY,
    last_seen_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('ready', 'unavailable'))
);


-- Bind the capability module to the immutable customer-approved session
-- payload. The raw approval token and capability secret remain absent here.
CREATE OR REPLACE FUNCTION erpx_control.guard_support_capability_binding()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    request_row erpx_control.support_access_requests%ROWTYPE;
    approved_module TEXT;
BEGIN
    SELECT * INTO request_row
    FROM erpx_control.support_access_requests
    WHERE id = NEW.access_request_id;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'support capability requires an existing access request';
    END IF;

    IF request_row.approved_at IS NULL OR request_row.status <> 'approved' THEN
        RAISE EXCEPTION 'support capability requires an approved access request';
    END IF;

    approved_module := COALESCE(request_row.payload->>'module', 'support');
    IF NEW.ticket_id IS DISTINCT FROM request_row.ticket_id
       OR NEW.customer_account_id IS DISTINCT FROM request_row.customer_account_id
       OR NEW.agent_ref IS DISTINCT FROM request_row.agent_ref
       OR NEW.mode IS DISTINCT FROM request_row.mode
       OR NEW.permissions IS DISTINCT FROM request_row.permissions
       OR NEW.scope IS DISTINCT FROM request_row.scope
       OR NEW.module IS DISTINCT FROM approved_module
       OR NEW.issued_at < request_row.approved_at
       OR NEW.expires_at > request_row.expires_at
    THEN
        RAISE EXCEPTION 'support capability binding exceeds customer approval';
    END IF;

    IF TG_OP = 'UPDATE' AND (
        OLD.access_request_id IS DISTINCT FROM NEW.access_request_id
        OR OLD.customer_account_id IS DISTINCT FROM NEW.customer_account_id
        OR OLD.agent_ref IS DISTINCT FROM NEW.agent_ref
        OR OLD.device_ref IS DISTINCT FROM NEW.device_ref
        OR OLD.ticket_id IS DISTINCT FROM NEW.ticket_id
        OR OLD.mode IS DISTINCT FROM NEW.mode
        OR OLD.permissions IS DISTINCT FROM NEW.permissions
        OR OLD.scope IS DISTINCT FROM NEW.scope
        OR OLD.module IS DISTINCT FROM NEW.module
        OR OLD.issued_at IS DISTINCT FROM NEW.issued_at
        OR OLD.expires_at IS DISTINCT FROM NEW.expires_at
        OR OLD.secret_hash IS DISTINCT FROM NEW.secret_hash
        OR OLD.reissued_from IS DISTINCT FROM NEW.reissued_from
        OR OLD.payload IS DISTINCT FROM NEW.payload
    ) THEN
        RAISE EXCEPTION 'support capability approval binding is immutable';
    END IF;

    IF TG_OP = 'UPDATE' AND OLD.revoked_at IS NOT NULL
       AND (NEW.revoked_at IS NULL OR NEW.status <> 'revoked')
    THEN
        RAISE EXCEPTION 'support capability revocation is immutable';
    END IF;

    RETURN NEW;
END;
$$;


-- Durable control-plane replay budget for Stripe dead-letter events.
-- Existing rows begin with no manual replay history; future replays are
-- bounded by the infrastructure policy and this monotonic counter.
ALTER TABLE erpx_control.stripe_event_inbox
    ADD COLUMN IF NOT EXISTS replay_count INTEGER NOT NULL DEFAULT 0;

ALTER TABLE erpx_control.stripe_event_inbox
    DROP CONSTRAINT IF EXISTS stripe_event_inbox_replay_count_nonnegative;

ALTER TABLE erpx_control.stripe_event_inbox
    ADD CONSTRAINT stripe_event_inbox_replay_count_nonnegative
    CHECK (replay_count >= 0);


-- G2-005 durable role-review projections.
--
-- These tables are derived review state only. The authoritative role
-- assignment aggregate remains erpx_control.role_assignments. There are no
-- credential, token, secret, or generic raw-payload columns in this schema.
-- Rollback: stop review persistence, verify no worker claim is in flight, then
-- drop the tables in reverse dependency order in a separately approved
-- migration. Never delete role_assignments as part of this rollback.

CREATE TABLE IF NOT EXISTS erpx_control.role_review_snapshots (
    snapshot_key TEXT PRIMARY KEY,
    scope_key TEXT NOT NULL,
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('platform', 'holding', 'company', 'branch', 'subtree')),
    holding_id UUID,
    company_id UUID,
    branch_id UUID,
    scope_company_ids UUID[] NOT NULL DEFAULT '{}',
    scope_branch_ids UUID[] NOT NULL DEFAULT '{}',
    evaluated_at TIMESTAMPTZ NOT NULL,
    expiry_horizon_until TIMESTAMPTZ NOT NULL,
    snapshot_version BIGINT NOT NULL CHECK (snapshot_version > 0),
    content_hash TEXT NOT NULL CHECK (content_hash ~ '^[0-9a-f]{64}$'),
    item_count INTEGER NOT NULL CHECK (item_count >= 0 AND item_count <= 1000),
    actor_ref TEXT NOT NULL CHECK (length(btrim(actor_ref)) > 0),
    correlation_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT role_review_snapshots_scope_shape CHECK (
        (scope_kind = 'platform'
            AND holding_id IS NULL AND company_id IS NULL AND branch_id IS NULL
            AND cardinality(scope_company_ids) = 0
            AND cardinality(scope_branch_ids) = 0)
        OR (scope_kind = 'holding'
            AND holding_id IS NOT NULL AND company_id IS NULL AND branch_id IS NULL
            AND cardinality(scope_company_ids) = 0
            AND cardinality(scope_branch_ids) = 0)
        OR (scope_kind = 'company'
            AND holding_id IS NOT NULL AND company_id IS NOT NULL AND branch_id IS NULL
            AND cardinality(scope_company_ids) = 1
            AND cardinality(scope_branch_ids) = 0)
        OR (scope_kind = 'branch'
            AND holding_id IS NOT NULL AND company_id IS NOT NULL AND branch_id IS NOT NULL
            AND cardinality(scope_company_ids) = 1
            AND cardinality(scope_branch_ids) = 1)
        OR (scope_kind = 'subtree'
            AND holding_id IS NOT NULL AND company_id IS NULL AND branch_id IS NULL
            AND cardinality(scope_company_ids) >= 1
            AND cardinality(scope_branch_ids) >= 0)
    ),
    CONSTRAINT role_review_snapshots_horizon_after_evaluation
        CHECK (expiry_horizon_until > evaluated_at)
);

CREATE INDEX IF NOT EXISTS role_review_snapshots_scope_idx
    ON erpx_control.role_review_snapshots (scope_kind, holding_id, company_id, branch_id, evaluated_at DESC);

CREATE INDEX IF NOT EXISTS role_review_snapshots_updated_idx
    ON erpx_control.role_review_snapshots (updated_at DESC, snapshot_key);

CREATE TABLE IF NOT EXISTS erpx_control.role_review_snapshot_idempotency (
    idempotency_key TEXT PRIMARY KEY CHECK (length(btrim(idempotency_key)) > 0),
    snapshot_key TEXT NOT NULL REFERENCES erpx_control.role_review_snapshots(snapshot_key) ON DELETE CASCADE,
    request_hash TEXT NOT NULL CHECK (request_hash ~ '^[0-9a-f]{64}$'),
    snapshot_version BIGINT NOT NULL CHECK (snapshot_version > 0),
    item_count INTEGER NOT NULL CHECK (item_count >= 0 AND item_count <= 1000),
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS role_review_snapshot_idempotency_snapshot_idx
    ON erpx_control.role_review_snapshot_idempotency (snapshot_key, recorded_at DESC);

CREATE TABLE IF NOT EXISTS erpx_control.role_review_items (
    item_key TEXT PRIMARY KEY,
    snapshot_key TEXT NOT NULL REFERENCES erpx_control.role_review_snapshots(snapshot_key) ON DELETE CASCADE,
    assignment_id UUID NOT NULL,
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('platform', 'holding', 'company', 'branch', 'subtree')),
    holding_id UUID,
    company_id UUID,
    branch_id UUID,
    scope_company_ids UUID[] NOT NULL DEFAULT '{}',
    scope_branch_ids UUID[] NOT NULL DEFAULT '{}',
    principal_ref TEXT NOT NULL CHECK (length(btrim(principal_ref)) > 0),
    valid_from TIMESTAMPTZ NOT NULL,
    valid_until TIMESTAMPTZ NOT NULL,
    parent_assignment_id UUID,
    assignment_version INTEGER NOT NULL CHECK (assignment_version > 0),
    classifications TEXT[] NOT NULL CHECK (cardinality(classifications) >= 1),
    notification_kind TEXT CHECK (notification_kind IS NULL OR notification_kind IN ('expiring_soon', 'expired')),
    notification_key TEXT,
    role_key TEXT NOT NULL CHECK (length(btrim(role_key)) > 0),
    permissions TEXT[] NOT NULL,
    functional_domain TEXT NOT NULL CHECK (length(btrim(functional_domain)) > 0),
    authority_level SMALLINT NOT NULL CHECK (authority_level >= 0),
    approval_limit_minor BIGINT NOT NULL CHECK (approval_limit_minor >= 0),
    timezone TEXT NOT NULL CHECK (length(btrim(timezone)) > 0),
    risk_tier TEXT NOT NULL CHECK (risk_tier IN ('platform_root', 'administrator', 'high', 'standard', 'contractor', 'support_diagnostic', 'support_mutation', 'emergency')),
    CONSTRAINT role_review_items_window_order CHECK (valid_until > valid_from),
    CONSTRAINT role_review_items_notification_pair CHECK (
        (notification_kind IS NULL AND notification_key IS NULL)
        OR (notification_kind IS NOT NULL AND notification_key IS NOT NULL)
    ),
    UNIQUE (snapshot_key, assignment_id)
);

CREATE INDEX IF NOT EXISTS role_review_items_snapshot_idx
    ON erpx_control.role_review_items (snapshot_key, assignment_id);

CREATE TABLE IF NOT EXISTS erpx_control.role_expiry_notifications (
    notification_key TEXT PRIMARY KEY,
    snapshot_key TEXT NOT NULL REFERENCES erpx_control.role_review_snapshots(snapshot_key) ON DELETE CASCADE,
    assignment_id UUID NOT NULL,
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('platform', 'holding', 'company', 'branch', 'subtree')),
    holding_id UUID,
    company_id UUID,
    branch_id UUID,
    scope_company_ids UUID[] NOT NULL DEFAULT '{}',
    scope_branch_ids UUID[] NOT NULL DEFAULT '{}',
    principal_ref TEXT NOT NULL CHECK (length(btrim(principal_ref)) > 0),
    valid_until TIMESTAMPTZ NOT NULL,
    notification_kind TEXT NOT NULL CHECK (notification_kind IN ('expiring_soon', 'expired')),
    status TEXT NOT NULL CHECK (status IN ('pending', 'claimed', 'reconciled', 'superseded')),
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    claim_key TEXT,
    claimed_by UUID,
    claimed_at TIMESTAMPTZ,
    lease_until TIMESTAMPTZ,
    reconciled_at TIMESTAMPTZ,
    version BIGINT NOT NULL DEFAULT 1 CHECK (version > 0),
    CONSTRAINT role_expiry_notifications_claim_shape CHECK (
        (status = 'pending' AND claimed_by IS NULL AND claimed_at IS NULL AND lease_until IS NULL)
        OR (status IN ('claimed', 'reconciled', 'superseded'))
    ),
    UNIQUE (snapshot_key, assignment_id, notification_kind)
);

CREATE INDEX IF NOT EXISTS role_expiry_notifications_claim_idx
    ON erpx_control.role_expiry_notifications (scope_kind, holding_id, company_id, branch_id, valid_until, notification_key)
    WHERE status IN ('pending', 'claimed') AND reconciled_at IS NULL;

CREATE INDEX IF NOT EXISTS role_expiry_notifications_claim_key_idx
    ON erpx_control.role_expiry_notifications (claim_key, notification_key)
    WHERE claim_key IS NOT NULL;

CREATE TABLE IF NOT EXISTS erpx_control.role_review_notification_claims (
    claim_key TEXT PRIMARY KEY CHECK (length(btrim(claim_key)) > 0),
    scope_key TEXT NOT NULL,
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('platform', 'holding', 'company', 'branch', 'subtree')),
    holding_id UUID,
    company_id UUID,
    branch_id UUID,
    scope_company_ids UUID[] NOT NULL DEFAULT '{}',
    scope_branch_ids UUID[] NOT NULL DEFAULT '{}',
    claimant_ref TEXT NOT NULL CHECK (length(btrim(claimant_ref)) > 0),
    requested_limit INTEGER NOT NULL CHECK (requested_limit > 0 AND requested_limit <= 100),
    notification_keys TEXT[] NOT NULL,
    claimed_at TIMESTAMPTZ NOT NULL,
    lease_until TIMESTAMPTZ NOT NULL CHECK (lease_until > claimed_at),
    reconciled_count INTEGER NOT NULL DEFAULT 0 CHECK (reconciled_count >= 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS role_review_notification_claims_scope_idx
    ON erpx_control.role_review_notification_claims (scope_kind, holding_id, company_id, branch_id, claimed_at DESC);

CREATE TABLE IF NOT EXISTS erpx_control.role_recovery_review_receipts (
    request_id UUID NOT NULL,
    review_version BIGINT NOT NULL CHECK (review_version > 0),
    scope_key TEXT NOT NULL,
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('platform', 'holding', 'company', 'branch', 'subtree')),
    holding_id UUID,
    company_id UUID,
    branch_id UUID,
    scope_company_ids UUID[] NOT NULL DEFAULT '{}',
    scope_branch_ids UUID[] NOT NULL DEFAULT '{}',
    target_kind TEXT NOT NULL CHECK (target_kind IN ('platform_root', 'holding_owner')),
    target_holding_id UUID,
    principal_ref TEXT NOT NULL CHECK (length(btrim(principal_ref)) > 0),
    permissions TEXT[] NOT NULL CHECK (cardinality(permissions) >= 1),
    functional_domain TEXT NOT NULL CHECK (length(btrim(functional_domain)) > 0),
    authority_level SMALLINT NOT NULL CHECK (authority_level >= 0),
    valid_from TIMESTAMPTZ NOT NULL,
    valid_until TIMESTAMPTZ NOT NULL,
    timezone TEXT NOT NULL CHECK (length(btrim(timezone)) > 0),
    reason_hash TEXT NOT NULL CHECK (reason_hash ~ '^[0-9a-f]{64}$'),
    ticket_reference_hash TEXT CHECK (ticket_reference_hash IS NULL OR ticket_reference_hash ~ '^[0-9a-f]{64}$'),
    requester_ref TEXT NOT NULL CHECK (length(btrim(requester_ref)) > 0),
    approver_ref TEXT NOT NULL CHECK (length(btrim(approver_ref)) > 0),
    reviewer_ref TEXT,
    decision TEXT NOT NULL CHECK (decision IN ('approved', 'rejected')),
    request_hash TEXT NOT NULL CHECK (request_hash ~ '^[0-9a-f]{64}$'),
    idempotency_key TEXT NOT NULL UNIQUE CHECK (length(btrim(idempotency_key)) > 0),
    actor_ref TEXT NOT NULL CHECK (length(btrim(actor_ref)) > 0),
    correlation_id UUID NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (request_id, review_version),
    CONSTRAINT role_recovery_receipt_target_shape CHECK (
        (target_kind = 'platform_root' AND target_holding_id IS NULL)
        OR (target_kind = 'holding_owner' AND target_holding_id IS NOT NULL)
    ),
    CONSTRAINT role_recovery_receipt_window_order CHECK (valid_until > valid_from)
);

CREATE INDEX IF NOT EXISTS role_recovery_review_receipts_scope_idx
    ON erpx_control.role_recovery_review_receipts (scope_kind, holding_id, company_id, branch_id, recorded_at DESC);

CREATE INDEX IF NOT EXISTS role_recovery_review_receipts_request_idx
    ON erpx_control.role_recovery_review_receipts (request_id, review_version DESC);


-- G4-002 support attachment metadata sidecar.
-- This table stores metadata only. Object bytes, credentials, signed URLs, and
-- capability secrets remain in object storage or the support capability store.

ALTER TABLE erpx_control.support_command_idempotency
    DROP CONSTRAINT IF EXISTS support_command_idempotency_command_type_check;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'support_command_idempotency_command_type_check'
          AND conrelid = 'erpx_control.support_command_idempotency'::regclass
    ) THEN
        ALTER TABLE erpx_control.support_command_idempotency
            ADD CONSTRAINT support_command_idempotency_command_type_check CHECK (
                command_type IN (
                    'ticket_insert', 'ticket_update',
                    'access_request_insert', 'access_request_approve',
                    'capability_issue', 'capability_revoke',
                    'incident_insert', 'incident_update',
                    'attachment_insert', 'attachment_revoke', 'attachment_expire'
                )
            );
    END IF;
END $$;

CREATE TABLE IF NOT EXISTS erpx_control.support_attachments (
    id UUID PRIMARY KEY,
    ticket_id UUID NOT NULL
        REFERENCES erpx_control.support_tickets(id) ON DELETE RESTRICT,
    customer_account_id UUID NOT NULL,
    object_ref TEXT NOT NULL CHECK (
        btrim(object_ref) <> '' AND length(object_ref) <= 512
    ),
    content_type TEXT NOT NULL CHECK (
        btrim(content_type) <> '' AND length(content_type) <= 255
    ),
    content_size_bytes BIGINT NOT NULL CHECK (
        content_size_bytes > 0 AND content_size_bytes <= 52428800
    ),
    sha256_hex TEXT NOT NULL CHECK (
        length(sha256_hex) = 64 AND sha256_hex ~ '^[0-9A-Fa-f]{64}$'
    ),
    visibility TEXT NOT NULL CHECK (
        visibility IN ('customer_visible', 'internal')
    ),
    uploader_ref TEXT NOT NULL CHECK (
        btrim(uploader_ref) <> '' AND length(uploader_ref) <= 255
    ),
    scope JSONB NOT NULL CHECK (
        scope = '"Platform"'::jsonb
        OR (
            jsonb_typeof(scope) = 'object'
            AND (
                (
                    scope ? 'Holding'
                    AND jsonb_typeof(scope->'Holding') = 'object'
                    AND scope->'Holding' ? 'holding_id'
                )
                OR (
                    scope ? 'Company'
                    AND jsonb_typeof(scope->'Company') = 'object'
                    AND scope->'Company' ? 'holding_id'
                    AND scope->'Company' ? 'company_id'
                )
                OR (
                    scope ? 'Branch'
                    AND jsonb_typeof(scope->'Branch') = 'object'
                    AND scope->'Branch' ? 'holding_id'
                    AND scope->'Branch' ? 'company_id'
                    AND scope->'Branch' ? 'branch_id'
                )
                OR (
                    scope ? 'Subtree'
                    AND jsonb_typeof(scope->'Subtree') = 'object'
                    AND scope->'Subtree' ? 'holding_id'
                    AND jsonb_typeof(scope->'Subtree'->'company_ids') = 'array'
                    AND jsonb_typeof(scope->'Subtree'->'branch_ids') = 'array'
                )
            )
        )
    ),
    uploaded_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('active', 'revoked', 'expired')),
    revoked_at TIMESTAMPTZ,
    version BIGINT NOT NULL CHECK (version > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (expires_at > uploaded_at),
    CHECK (expires_at <= uploaded_at + interval '365 days'),
    CHECK (
        (status IN ('active', 'expired') AND revoked_at IS NULL)
        OR (status = 'revoked' AND revoked_at IS NOT NULL)
    ),
    UNIQUE (ticket_id, object_ref)
);

CREATE INDEX IF NOT EXISTS support_attachments_ticket_active_idx
    ON erpx_control.support_attachments (ticket_id, uploaded_at DESC, id)
    WHERE status = 'active';

CREATE INDEX IF NOT EXISTS support_attachments_customer_active_idx
    ON erpx_control.support_attachments
        (customer_account_id, expires_at, ticket_id)
    WHERE status = 'active';

CREATE OR REPLACE FUNCTION erpx_control.guard_support_attachment_binding()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    ticket_customer UUID;
    ticket_status TEXT;
BEGIN
    SELECT customer_account_id, status
      INTO ticket_customer, ticket_status
      FROM erpx_control.support_tickets
     WHERE id = NEW.ticket_id;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'support attachment requires an existing ticket';
    END IF;

    IF NEW.customer_account_id IS DISTINCT FROM ticket_customer THEN
        RAISE EXCEPTION 'support attachment customer binding does not match ticket';
    END IF;

    IF TG_OP = 'INSERT' AND ticket_status = 'closed' THEN
        RAISE EXCEPTION 'closed support tickets cannot receive attachments';
    END IF;

    IF TG_OP = 'UPDATE' THEN
        IF OLD.ticket_id IS DISTINCT FROM NEW.ticket_id
           OR OLD.customer_account_id IS DISTINCT FROM NEW.customer_account_id
           OR OLD.object_ref IS DISTINCT FROM NEW.object_ref
           OR OLD.content_type IS DISTINCT FROM NEW.content_type
           OR OLD.content_size_bytes IS DISTINCT FROM NEW.content_size_bytes
           OR OLD.sha256_hex IS DISTINCT FROM NEW.sha256_hex
           OR OLD.visibility IS DISTINCT FROM NEW.visibility
           OR OLD.uploader_ref IS DISTINCT FROM NEW.uploader_ref
           OR OLD.scope IS DISTINCT FROM NEW.scope
           OR OLD.uploaded_at IS DISTINCT FROM NEW.uploaded_at
           OR OLD.expires_at IS DISTINCT FROM NEW.expires_at
        THEN
            RAISE EXCEPTION 'support attachment binding is immutable';
        END IF;

        IF OLD.status <> 'active' THEN
            RAISE EXCEPTION 'support attachment status is terminal';
        END IF;

        IF NEW.status = 'revoked' AND ticket_status = 'closed' THEN
            RAISE EXCEPTION 'closed support tickets cannot revoke attachments';
        END IF;
    END IF;

    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS support_attachment_binding_guard
    ON erpx_control.support_attachments;
CREATE TRIGGER support_attachment_binding_guard
BEFORE INSERT OR UPDATE ON erpx_control.support_attachments
FOR EACH ROW EXECUTE FUNCTION erpx_control.guard_support_attachment_binding();


-- Durable, replay-safe support SLA notification inbox. Delivery providers are
-- downstream; this table only owns the notification claim and lease state.
CREATE TABLE IF NOT EXISTS erpx_control.support_sla_notifications (
    notification_key TEXT PRIMARY KEY CHECK (btrim(notification_key) <> ''),
    ticket_id UUID NOT NULL REFERENCES erpx_control.support_tickets(id) ON DELETE RESTRICT,
    customer_account_id UUID NOT NULL,
    classification TEXT NOT NULL CHECK (classification IN ('deadline_approaching', 'deadline_breached')),
    audience TEXT NOT NULL CHECK (audience IN ('customer_visible', 'internal')),
    deadline TIMESTAMPTZ NOT NULL,
    not_before TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'claimed', 'delivered', 'expired')),
    claimed_at TIMESTAMPTZ,
    claim_expires_at TIMESTAMPTZ,
    delivered_at TIMESTAMPTZ,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (not_before < expires_at),
    CHECK (claim_expires_at IS NULL OR claimed_at IS NOT NULL),
    CHECK ((status = 'claimed') = (claimed_at IS NOT NULL AND claim_expires_at IS NOT NULL)),
    CHECK ((status = 'delivered') = (delivered_at IS NOT NULL))
);

CREATE INDEX IF NOT EXISTS support_sla_notifications_due_idx
    ON erpx_control.support_sla_notifications
       (status, not_before, expires_at, notification_key);


-- Durable control-plane scheduling for bounded, holding-routed projection work.
-- These rows coordinate workers only; aggregate and read-model state remains in
-- the holding write/read databases.

CREATE TABLE IF NOT EXISTS erpx_control.holding_projection_jobs (
    id UUID PRIMARY KEY,
    holding_id TEXT NOT NULL CHECK (length(trim(holding_id)) BETWEEN 1 AND 200),
    data_profile_key TEXT NOT NULL CHECK (length(trim(data_profile_key)) BETWEEN 1 AND 200),
    projection_name TEXT NOT NULL CHECK (
        length(trim(projection_name)) BETWEEN 1 AND 120
        AND projection_name ~ '^[A-Za-z0-9_.-]+$'
    ),
    job_key TEXT NOT NULL CHECK (length(trim(job_key)) BETWEEN 1 AND 200),
    kind TEXT NOT NULL CHECK (kind IN ('incremental', 'rebuild')),
    state TEXT NOT NULL CHECK (
        state IN (
            'pending', 'claimed', 'retryable', 'succeeded', 'dead_letter',
            'rebuild_pending', 'rebuild_succeeded'
        )
    ),
    available_at TIMESTAMPTZ NOT NULL,
    page_size INTEGER NOT NULL CHECK (page_size BETWEEN 1 AND 500),
    batch_size INTEGER NOT NULL CHECK (batch_size BETWEEN 1 AND 100),
    max_attempts INTEGER NOT NULL CHECK (max_attempts BETWEEN 1 AND 25),
    backoff_base_seconds BIGINT NOT NULL CHECK (backoff_base_seconds BETWEEN 1 AND 86400),
    backoff_max_seconds BIGINT NOT NULL CHECK (
        backoff_max_seconds BETWEEN 1 AND 86400
        AND backoff_max_seconds >= backoff_base_seconds
    ),
    payload JSONB NOT NULL DEFAULT '{}'::jsonb
        CHECK (octet_length(payload::text) <= 65536),
    request_hash TEXT NOT NULL CHECK (length(request_hash) = 64),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts BETWEEN 0 AND 25),
    current_claim_id UUID,
    worker_id UUID,
    lease_until TIMESTAMPTZ,
    last_error TEXT CHECK (last_error IS NULL OR octet_length(last_error) <= 1000),
    dead_lettered_at TIMESTAMPTZ,
    rebuild_id UUID,
    version BIGINT NOT NULL DEFAULT 1 CHECK (version > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (holding_id, data_profile_key, projection_name, job_key),
    CONSTRAINT holding_projection_claim_fields_consistent CHECK (
        (state = 'claimed') = (
            current_claim_id IS NOT NULL AND worker_id IS NOT NULL AND lease_until IS NOT NULL
        )
    ),
    CONSTRAINT holding_projection_rebuild_kind_consistent CHECK (
        (kind = 'rebuild') OR state NOT IN ('rebuild_pending', 'rebuild_succeeded')
    )
);

CREATE INDEX IF NOT EXISTS holding_projection_jobs_due_idx
    ON erpx_control.holding_projection_jobs (holding_id, data_profile_key, available_at, id)
    WHERE state IN ('pending', 'retryable', 'rebuild_pending');

CREATE INDEX IF NOT EXISTS holding_projection_jobs_stale_idx
    ON erpx_control.holding_projection_jobs (holding_id, data_profile_key, lease_until, id)
    WHERE state = 'claimed';

CREATE TABLE IF NOT EXISTS erpx_control.holding_projection_claim_batches (
    claim_key TEXT PRIMARY KEY CHECK (length(trim(claim_key)) BETWEEN 1 AND 200),
    holding_id TEXT NOT NULL CHECK (length(trim(holding_id)) BETWEEN 1 AND 200),
    data_profile_key TEXT NOT NULL CHECK (length(trim(data_profile_key)) BETWEEN 1 AND 200),
    worker_id UUID NOT NULL,
    sealed BOOLEAN NOT NULL DEFAULT false,
    result_count INTEGER NOT NULL DEFAULT 0 CHECK (result_count BETWEEN 0 AND 100),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS holding_projection_claim_batches_scope_idx
    ON erpx_control.holding_projection_claim_batches (holding_id, data_profile_key, created_at DESC);

CREATE TABLE IF NOT EXISTS erpx_control.holding_projection_claims (
    claim_id UUID PRIMARY KEY,
    claim_key TEXT NOT NULL REFERENCES erpx_control.holding_projection_claim_batches (claim_key),
    job_id UUID NOT NULL REFERENCES erpx_control.holding_projection_jobs (id),
    holding_id TEXT NOT NULL CHECK (length(trim(holding_id)) BETWEEN 1 AND 200),
    data_profile_key TEXT NOT NULL CHECK (length(trim(data_profile_key)) BETWEEN 1 AND 200),
    worker_id UUID NOT NULL,
    claimed_at TIMESTAMPTZ NOT NULL,
    lease_until TIMESTAMPTZ NOT NULL CHECK (lease_until > claimed_at),
    state TEXT NOT NULL CHECK (state IN ('active', 'completed', 'stale', 'failed', 'dead_letter')),
    completed_at TIMESTAMPTZ,
    outcome TEXT,
    UNIQUE (claim_key, job_id)
);

CREATE UNIQUE INDEX IF NOT EXISTS holding_projection_active_job_claim_idx
    ON erpx_control.holding_projection_claims (job_id)
    WHERE state = 'active';

CREATE INDEX IF NOT EXISTS holding_projection_claims_worker_idx
    ON erpx_control.holding_projection_claims (holding_id, data_profile_key, worker_id, state, lease_until);

CREATE TABLE IF NOT EXISTS erpx_control.holding_projection_rebuilds (
    id UUID PRIMARY KEY,
    holding_id TEXT NOT NULL CHECK (length(trim(holding_id)) BETWEEN 1 AND 200),
    data_profile_key TEXT NOT NULL CHECK (length(trim(data_profile_key)) BETWEEN 1 AND 200),
    projection_name TEXT NOT NULL CHECK (
        length(trim(projection_name)) BETWEEN 1 AND 120
        AND projection_name ~ '^[A-Za-z0-9_.-]+$'
    ),
    rebuild_key TEXT NOT NULL CHECK (length(trim(rebuild_key)) BETWEEN 1 AND 200),
    job_id UUID NOT NULL REFERENCES erpx_control.holding_projection_jobs (id),
    state TEXT NOT NULL CHECK (state IN ('pending', 'running', 'succeeded', 'dead_letter')),
    payload JSONB NOT NULL DEFAULT '{}'::jsonb
        CHECK (octet_length(payload::text) <= 65536),
    requested_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (holding_id, data_profile_key, projection_name, rebuild_key)
);

CREATE INDEX IF NOT EXISTS holding_projection_rebuilds_scope_idx
    ON erpx_control.holding_projection_rebuilds (holding_id, data_profile_key, state, updated_at DESC);


-- Provider-account routing metadata. Secrets stay in the deployment secret
-- manager; this table stores only opaque references and authoritative holding
-- binding data used before a callback reaches a holding database.
CREATE TABLE IF NOT EXISTS erpx_control.marketing_provider_accounts (
    provider_account_id UUID PRIMARY KEY,
    provider_account_ref TEXT NOT NULL CHECK (btrim(provider_account_ref) <> ''),
    holding_id UUID NOT NULL,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    verifier_key_ref TEXT NOT NULL CHECK (btrim(verifier_key_ref) <> ''),
    state TEXT NOT NULL CHECK (state IN ('active', 'suspended')),
    version BIGINT NOT NULL CHECK (version > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (provider_account_ref)
);

CREATE INDEX IF NOT EXISTS marketing_provider_accounts_route_idx
    ON erpx_control.marketing_provider_accounts
       (provider_account_ref, holding_id, data_profile_key, state);


-- Control-plane-only routing metadata for Finance bank-provider accounts.
-- Provider SDK payloads, access tokens, and credential bytes remain outside
-- PostgreSQL in the deployment credential manager. This table stores only
-- opaque references needed to route a validated provider operation.
CREATE TABLE IF NOT EXISTS erpx_control.finance_bank_provider_accounts (
    finance_bank_provider_account_id UUID PRIMARY KEY,
    provider_account_ref TEXT NOT NULL
        CHECK (length(btrim(provider_account_ref)) BETWEEN 1 AND 255),
    provider_key TEXT NOT NULL
        CHECK (length(btrim(provider_key)) BETWEEN 1 AND 255),
    holding_id UUID NOT NULL,
    data_profile_key TEXT NOT NULL
        CHECK (length(btrim(data_profile_key)) BETWEEN 1 AND 255),
        CHECK (holding_id <> '00000000-0000-0000-0000-000000000000'::uuid),
    credential_key_ref TEXT NOT NULL
        CHECK (length(btrim(credential_key_ref)) BETWEEN 1 AND 255),
    state TEXT NOT NULL CHECK (state IN ('active', 'suspended')),
    version BIGINT NOT NULL CHECK (version > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (provider_account_ref)
);

CREATE INDEX IF NOT EXISTS finance_bank_provider_accounts_route_idx
    ON erpx_control.finance_bank_provider_accounts
       (provider_account_ref, provider_key, holding_id, data_profile_key,
        credential_key_ref, state);


-- Additive outbound routing metadata for Marketing provider accounts.
-- Existing rows remain routable for callback verification while the
-- deployment resolves the legacy placeholders as unconfigured outbound
-- routes. No credential material is stored in PostgreSQL.
ALTER TABLE erpx_control.marketing_provider_accounts
    ADD COLUMN IF NOT EXISTS provider_key TEXT NOT NULL
        DEFAULT 'legacy-unconfigured'
        CHECK (length(btrim(provider_key)) BETWEEN 1 AND 255);

ALTER TABLE erpx_control.marketing_provider_accounts
    ADD COLUMN IF NOT EXISTS outbound_credential_ref TEXT NOT NULL
        DEFAULT 'legacy-unconfigured-credential'
        CHECK (length(btrim(outbound_credential_ref)) BETWEEN 1 AND 255);


-- Durable anonymous Marketing abuse windows. The key is intentionally scoped
-- to the public holding route and published resource; no IP, contact, token,
-- or request-body value is stored.
CREATE TABLE IF NOT EXISTS erpx_control.marketing_public_abuse_windows (
    holding_id TEXT NOT NULL REFERENCES erpx_control.holding_registry(holding_id) ON DELETE CASCADE,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    hostname TEXT NOT NULL CHECK (length(btrim(hostname)) BETWEEN 1 AND 253),
    resource_kind TEXT NOT NULL CHECK (resource_kind IN ('form', 'preview')),
    resource_id UUID NOT NULL,
    window_started_at TIMESTAMPTZ NOT NULL,
    request_count BIGINT NOT NULL CHECK (request_count > 0),
    last_seen_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, data_profile_key, hostname, resource_kind, resource_id, window_started_at)
);

CREATE INDEX IF NOT EXISTS marketing_public_abuse_cleanup_idx
    ON erpx_control.marketing_public_abuse_windows (last_seen_at);


-- Durable control-plane coordination for the Marketing delivery pass.
-- Delivery state remains authoritative in each holding write database; these
-- rows only lease a bounded global worker pass and retain retry/DLQ history.

CREATE TABLE IF NOT EXISTS erpx_control.marketing_delivery_scheduler_jobs (
    id UUID PRIMARY KEY,
    job_key TEXT NOT NULL UNIQUE CHECK (
        length(trim(job_key)) BETWEEN 1 AND 200
        AND job_key ~ '^[A-Za-z0-9_.:-]+$'
    ),
    state TEXT NOT NULL CHECK (
        state IN ('pending', 'claimed', 'retryable', 'dead_letter')
    ),
    available_at TIMESTAMPTZ NOT NULL,
    cadence_seconds BIGINT NOT NULL CHECK (cadence_seconds BETWEEN 1 AND 86400),
    max_attempts INTEGER NOT NULL CHECK (max_attempts BETWEEN 1 AND 25),
    backoff_base_seconds BIGINT NOT NULL CHECK (backoff_base_seconds BETWEEN 1 AND 86400),
    backoff_max_seconds BIGINT NOT NULL CHECK (
        backoff_max_seconds BETWEEN 1 AND 86400
        AND backoff_max_seconds >= backoff_base_seconds
    ),
    request_hash TEXT NOT NULL CHECK (length(request_hash) = 64),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts BETWEEN 0 AND 25),
    current_claim_id UUID,
    worker_id UUID,
    lease_until TIMESTAMPTZ,
    last_error TEXT CHECK (last_error IS NULL OR octet_length(last_error) <= 1000),
    dead_lettered_at TIMESTAMPTZ,
    last_succeeded_at TIMESTAMPTZ,
    version BIGINT NOT NULL DEFAULT 1 CHECK (version > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT marketing_delivery_scheduler_claim_fields_consistent CHECK (
        (state = 'claimed') = (
            current_claim_id IS NOT NULL AND worker_id IS NOT NULL AND lease_until IS NOT NULL
        )
    )
);

CREATE INDEX IF NOT EXISTS marketing_delivery_scheduler_due_idx
    ON erpx_control.marketing_delivery_scheduler_jobs (available_at, id)
    WHERE state IN ('pending', 'retryable');

CREATE INDEX IF NOT EXISTS marketing_delivery_scheduler_stale_idx
    ON erpx_control.marketing_delivery_scheduler_jobs (lease_until, id)
    WHERE state = 'claimed';

CREATE TABLE IF NOT EXISTS erpx_control.marketing_delivery_scheduler_claims (
    claim_id UUID PRIMARY KEY,
    job_id UUID NOT NULL REFERENCES erpx_control.marketing_delivery_scheduler_jobs(id),
    worker_id UUID NOT NULL,
    claimed_at TIMESTAMPTZ NOT NULL,
    lease_until TIMESTAMPTZ NOT NULL CHECK (lease_until > claimed_at),
    state TEXT NOT NULL CHECK (state IN ('active', 'completed', 'stale', 'failed', 'dead_letter')),
    completed_at TIMESTAMPTZ,
    outcome TEXT,
    UNIQUE (job_id, claim_id)
);

CREATE UNIQUE INDEX IF NOT EXISTS marketing_delivery_scheduler_active_claim_idx
    ON erpx_control.marketing_delivery_scheduler_claims (job_id)
    WHERE state = 'active';

CREATE INDEX IF NOT EXISTS marketing_delivery_scheduler_claim_worker_idx
    ON erpx_control.marketing_delivery_scheduler_claims (worker_id, state, lease_until);


-- Marketing provider onboarding is control-plane configuration only. Secret
-- values remain in the deployment secret manager; these columns contain only
-- opaque references and non-secret routing metadata.
ALTER TABLE erpx_control.marketing_provider_accounts
    ADD COLUMN IF NOT EXISTS onboarding_state TEXT NOT NULL DEFAULT 'verified'
        CHECK (onboarding_state IN ('pending', 'verified'));

ALTER TABLE erpx_control.marketing_provider_accounts
    ADD COLUMN IF NOT EXISTS channels TEXT[] NOT NULL DEFAULT ARRAY['email']::TEXT[]
        CHECK (
            cardinality(channels) BETWEEN 1 AND 5
            AND channels <@ ARRAY['email', 'sms', 'social', 'push', 'webhook']::TEXT[]
        );

ALTER TABLE erpx_control.marketing_provider_accounts
    ADD COLUMN IF NOT EXISTS last_verified_at TIMESTAMPTZ;

ALTER TABLE erpx_control.marketing_provider_accounts
    ADD COLUMN IF NOT EXISTS verification_reference_hash TEXT
        CHECK (verification_reference_hash IS NULL OR length(verification_reference_hash) = 64);

ALTER TABLE erpx_control.marketing_provider_accounts
    ADD CONSTRAINT marketing_provider_accounts_active_requires_verification
    CHECK (state <> 'active' OR onboarding_state = 'verified');

CREATE INDEX IF NOT EXISTS marketing_provider_accounts_onboarding_idx
    ON erpx_control.marketing_provider_accounts (holding_id, data_profile_key, onboarding_state, state);


-- Per-account, provider-neutral channel routes. These URLs contain no
-- credentials; secret values remain in the deployment secret manager.
ALTER TABLE erpx_control.marketing_provider_accounts
    ADD COLUMN IF NOT EXISTS channel_routes JSONB NOT NULL DEFAULT '[]'::jsonb
        CHECK (jsonb_typeof(channel_routes) = 'array');

CREATE INDEX IF NOT EXISTS marketing_provider_accounts_channel_routes_idx
    ON erpx_control.marketing_provider_accounts (holding_id, data_profile_key, state);


-- Lower priority values are selected first for a channel. The provider
-- reference remains the deterministic tie-breaker; this is routing metadata,
-- not a secret or a payment authority.
ALTER TABLE erpx_control.marketing_provider_accounts
    ADD COLUMN IF NOT EXISTS provider_priority INTEGER NOT NULL DEFAULT 100
        CHECK (provider_priority BETWEEN 1 AND 10_000);

CREATE INDEX IF NOT EXISTS marketing_provider_accounts_priority_idx
    ON erpx_control.marketing_provider_accounts
       (holding_id, data_profile_key, state, provider_priority, provider_account_ref);


-- Durable control-plane coordination for holding backup and restore-test work.
-- Aggregate/database artifacts remain outside this table; only bounded opaque
-- references and worker state are persisted here.
CREATE TABLE IF NOT EXISTS erpx_control.holding_backup_jobs (
    id UUID PRIMARY KEY,
    holding_id TEXT NOT NULL REFERENCES erpx_control.holding_registry(holding_id) ON DELETE RESTRICT,
    data_profile_key TEXT NOT NULL CHECK (length(trim(data_profile_key)) BETWEEN 1 AND 200),
    kind TEXT NOT NULL CHECK (kind IN ('full', 'restore_test')),
    backup_ref TEXT NOT NULL CHECK (length(trim(backup_ref)) BETWEEN 1 AND 200),
    restore_target_ref TEXT,
    candidate_read_database_key TEXT,
    source_version BIGINT CHECK (source_version IS NULL OR source_version > 0),
    source_projection TEXT NOT NULL DEFAULT 'operations' CHECK (
        source_projection IN ('operations', 'finance_banking', 'finance_subledger', 'finance_close')
    ),
    state TEXT NOT NULL CHECK (
        state IN ('pending', 'claimed', 'retryable', 'completed', 'failed', 'dead_letter')
    ),
    available_at TIMESTAMPTZ NOT NULL,
    max_attempts INTEGER NOT NULL CHECK (max_attempts BETWEEN 1 AND 5),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts BETWEEN 0 AND 5),
    worker_id UUID,
    claim_id UUID,
    lease_until TIMESTAMPTZ,
    last_error TEXT CHECK (last_error IS NULL OR octet_length(last_error) <= 1000),
    verified BOOLEAN NOT NULL DEFAULT FALSE,
    request_hash TEXT NOT NULL CHECK (length(request_hash) = 64),
    idempotency_key TEXT NOT NULL CHECK (length(trim(idempotency_key)) BETWEEN 1 AND 200),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((kind = 'full' AND restore_target_ref IS NULL AND candidate_read_database_key IS NULL AND source_version IS NULL)
        OR (kind = 'restore_test' AND restore_target_ref IS NOT NULL AND candidate_read_database_key IS NOT NULL AND source_version > 0)),
    CHECK ((state = 'claimed') = (claim_id IS NOT NULL AND worker_id IS NOT NULL AND lease_until IS NOT NULL)),
    UNIQUE (holding_id, idempotency_key)
);

CREATE INDEX IF NOT EXISTS holding_backup_jobs_due_idx
    ON erpx_control.holding_backup_jobs (holding_id, data_profile_key, available_at, id)
    WHERE state IN ('pending', 'retryable');

CREATE INDEX IF NOT EXISTS holding_backup_jobs_history_idx
    ON erpx_control.holding_backup_jobs (holding_id, created_at DESC, id DESC);


-- Durable, holding-bound backup-history retention actions.
-- Object references remain write-side data only; this table stores counts and
-- redacted state so a worker can replay the same action safely.
CREATE TABLE IF NOT EXISTS erpx_control.holding_backup_retention_actions (
    id UUID PRIMARY KEY,
    holding_id TEXT NOT NULL REFERENCES erpx_control.holding_registry(holding_id) ON DELETE RESTRICT,
    data_profile_key TEXT NOT NULL CHECK (length(trim(data_profile_key)) BETWEEN 1 AND 200),
    cutoff_at TIMESTAMPTZ NOT NULL,
    max_rows INTEGER NOT NULL CHECK (max_rows BETWEEN 1 AND 1000),
    dry_run BOOLEAN NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('pending', 'claimed', 'completed', 'failed')),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts BETWEEN 0 AND 3),
    max_attempts INTEGER NOT NULL CHECK (max_attempts BETWEEN 1 AND 3),
    worker_id UUID,
    claim_id UUID,
    lease_until TIMESTAMPTZ,
    jobs_selected INTEGER NOT NULL DEFAULT 0 CHECK (jobs_selected >= 0),
    jobs_deleted INTEGER NOT NULL DEFAULT 0 CHECK (jobs_deleted >= 0),
    runs_deleted INTEGER NOT NULL DEFAULT 0 CHECK (runs_deleted >= 0),
    object_refs_released INTEGER NOT NULL DEFAULT 0 CHECK (object_refs_released >= 0),
    remaining_eligible INTEGER NOT NULL DEFAULT 0 CHECK (remaining_eligible >= 0),
    error_present BOOLEAN NOT NULL DEFAULT FALSE,
    request_hash TEXT NOT NULL CHECK (length(request_hash) = 64),
    idempotency_key TEXT NOT NULL CHECK (length(trim(idempotency_key)) BETWEEN 1 AND 200),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ,
    CHECK ((state = 'claimed') = (claim_id IS NOT NULL AND worker_id IS NOT NULL AND lease_until IS NOT NULL)),
    CHECK (jobs_deleted <= jobs_selected),
    UNIQUE (holding_id, idempotency_key)
);

CREATE INDEX IF NOT EXISTS holding_backup_retention_actions_history_idx
    ON erpx_control.holding_backup_retention_actions (holding_id, created_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS holding_backup_retention_actions_claim_idx
    ON erpx_control.holding_backup_retention_actions (state, lease_until, updated_at);


-- Forward-only durable attribution for each scheduled backup attempt.
-- Kept separate from migration 0052 so upgrades never rewrite an applied
-- migration checksum.
CREATE TABLE IF NOT EXISTS erpx_control.holding_backup_job_runs (
    job_id UUID NOT NULL REFERENCES erpx_control.holding_backup_jobs(id) ON DELETE RESTRICT,
    attempt INTEGER NOT NULL CHECK (attempt BETWEEN 1 AND 5),
    run_id UUID NOT NULL REFERENCES erpx_control.holding_backup_runs(id) ON DELETE RESTRICT,
    linked_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (job_id, attempt),
    UNIQUE (run_id)
);

CREATE INDEX IF NOT EXISTS holding_backup_job_runs_run_idx
    ON erpx_control.holding_backup_job_runs (run_id);


-- Enforce the holding/holding pair at the database boundary as well as in the
-- repository. The composite reference prevents a valid holding paired with a
-- different holding from being persisted by an out-of-band writer.
CREATE UNIQUE INDEX IF NOT EXISTS holding_registry_holding_holding_uq
    ON erpx_control.holding_registry (holding_id, data_profile_key);

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'erpx_control.holding_backup_retention_actions'::regclass
          AND conname = 'holding_backup_retention_holding_holding_fk'
    ) THEN
        ALTER TABLE erpx_control.holding_backup_retention_actions
            ADD CONSTRAINT holding_backup_retention_holding_holding_fk
            FOREIGN KEY (holding_id, data_profile_key)
            REFERENCES erpx_control.holding_registry (holding_id, data_profile_key)
            ON DELETE RESTRICT;
    END IF;
END
$$;


-- Secret-free evidence for comparing retention-selected backup references with
-- the holding-bound object-store inventory. The write-side retention claim is
-- authoritative; this report is appendable/replayable worker evidence.
CREATE TABLE IF NOT EXISTS erpx_control.holding_backup_inventory_reports (
    id UUID PRIMARY KEY,
    retention_action_id UUID NOT NULL
        REFERENCES erpx_control.holding_backup_retention_actions(id) ON DELETE RESTRICT,
    holding_id TEXT NOT NULL,
    data_profile_key TEXT NOT NULL CHECK (length(trim(data_profile_key)) BETWEEN 1 AND 200),
    expected_objects INTEGER NOT NULL CHECK (expected_objects >= 0),
    observed_objects INTEGER NOT NULL CHECK (observed_objects >= 0),
    missing_objects INTEGER NOT NULL CHECK (missing_objects >= 0),
    unexpected_objects INTEGER NOT NULL CHECK (unexpected_objects >= 0),
    state TEXT NOT NULL CHECK (state IN ('matched', 'drifted', 'unavailable')),
    error_present BOOLEAN NOT NULL DEFAULT FALSE,
    checked_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (retention_action_id),
    CHECK (missing_objects <= expected_objects)
);

CREATE INDEX IF NOT EXISTS holding_backup_inventory_reports_history_idx
    ON erpx_control.holding_backup_inventory_reports
        (holding_id, data_profile_key, checked_at DESC, id DESC);


CREATE TABLE IF NOT EXISTS erpx_control.holding_owner_seeds (
    holding_id UUID PRIMARY KEY,
    data_profile_key TEXT NOT NULL,
    principal_ref TEXT NOT NULL,
    approver_ref TEXT NOT NULL,
    role_assignment_id UUID NOT NULL UNIQUE,
    state TEXT NOT NULL CHECK (state IN ('pending', 'seeded')),
    recovery_issued_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (btrim(data_profile_key) <> ''),
    CHECK (btrim(principal_ref) <> ''),
    CHECK (btrim(approver_ref) <> ''),
    CHECK (principal_ref <> approver_ref)
);

CREATE UNIQUE INDEX IF NOT EXISTS holding_owner_seeds_principal_scope_idx
    ON erpx_control.holding_owner_seeds (principal_ref, holding_id);

CREATE INDEX IF NOT EXISTS holding_owner_seeds_state_idx
    ON erpx_control.holding_owner_seeds (state, updated_at, holding_id);


-- Durable, append-only G13 migration execution evidence. These tables are
-- coordination and evidence state only; they never switch production command
-- ownership or mutate Legacy business data.
-- Production command ownership remains unchanged until explicit approval.

CREATE TABLE IF NOT EXISTS erpx_control.migration_runs (
    run_id UUID PRIMARY KEY,
    cohort TEXT NOT NULL CHECK (length(trim(cohort)) BETWEEN 1 AND 128),
    source_contract_hash TEXT NOT NULL CHECK (source_contract_hash ~ '^[0-9a-fA-F]{64}$'),
    id_namespace TEXT NOT NULL CHECK (length(trim(id_namespace)) BETWEEN 1 AND 128),
    phase TEXT NOT NULL CHECK (phase IN (
        'source_frozen', 'ids_mapped', 'write_backfilled', 'read_projected',
        'shadow_replaying', 'reconciled', 'read_only_pilot',
        'workflow_accepted', 'rollback_rehearsed', 'mutation_frozen',
        'final_delta_imported', 'erpx_command_owner', 'legacy_read_only',
        'retention_complete', 'legacy_retired'
    )),
    command_owner TEXT NOT NULL CHECK (command_owner IN ('legacy', 'erpx')),
    retention_until TIMESTAMPTZ,
    version BIGINT NOT NULL CHECK (version > 0),
    transitioned_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (cohort),
    CHECK (
        (phase IN ('erpx_command_owner', 'legacy_read_only', 'retention_complete', 'legacy_retired'))
        = (command_owner = 'erpx')
    )
);

CREATE INDEX IF NOT EXISTS migration_runs_phase_idx
    ON erpx_control.migration_runs (phase, updated_at DESC, run_id);

CREATE TABLE IF NOT EXISTS erpx_control.migration_transition_events (
    event_id UUID PRIMARY KEY,
    run_id UUID NOT NULL REFERENCES erpx_control.migration_runs(run_id) ON DELETE RESTRICT,
    from_phase TEXT NOT NULL,
    to_phase TEXT NOT NULL,
    command_owner TEXT NOT NULL CHECK (command_owner IN ('legacy', 'erpx')),
    actor_ref TEXT NOT NULL CHECK (length(trim(actor_ref)) BETWEEN 1 AND 200),
    correlation_id UUID NOT NULL,
    idempotency_key TEXT NOT NULL CHECK (length(trim(idempotency_key)) BETWEEN 1 AND 200),
    request_hash TEXT NOT NULL CHECK (request_hash ~ '^[0-9a-fA-F]{64}$'),
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (run_id, idempotency_key)
);

CREATE INDEX IF NOT EXISTS migration_transition_events_run_idx
    ON erpx_control.migration_transition_events (run_id, occurred_at, event_id);

CREATE TABLE IF NOT EXISTS erpx_control.migration_idempotency (
    operation TEXT NOT NULL CHECK (length(trim(operation)) BETWEEN 1 AND 100),
    idempotency_key TEXT NOT NULL CHECK (length(trim(idempotency_key)) BETWEEN 1 AND 200),
    request_hash TEXT NOT NULL CHECK (request_hash ~ '^[0-9a-fA-F]{64}$'),
    run_id UUID REFERENCES erpx_control.migration_runs(run_id) ON DELETE RESTRICT,
    response JSONB,
    state TEXT NOT NULL CHECK (state IN ('reserved', 'completed', 'failed')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ,
    PRIMARY KEY (operation, idempotency_key)
);

CREATE TABLE IF NOT EXISTS erpx_control.migration_work_items (
    work_id UUID PRIMARY KEY,
    run_id UUID NOT NULL REFERENCES erpx_control.migration_runs(run_id) ON DELETE RESTRICT,
    work_kind TEXT NOT NULL CHECK (work_kind IN (
        'backfill', 'project', 'shadow_replay', 'final_delta', 'reconcile', 'rollback'
    )),
    scope_key TEXT NOT NULL CHECK (length(trim(scope_key)) BETWEEN 1 AND 200),
    source_fence TEXT NOT NULL CHECK (length(trim(source_fence)) BETWEEN 1 AND 200),
    request_hash TEXT NOT NULL CHECK (request_hash ~ '^[0-9a-fA-F]{64}$'),
    state TEXT NOT NULL CHECK (state IN (
        'pending', 'claimed', 'retryable', 'succeeded', 'failed', 'dead_letter'
    )),
    available_at TIMESTAMPTZ NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts BETWEEN 0 AND 25),
    max_attempts INTEGER NOT NULL CHECK (max_attempts BETWEEN 1 AND 25),
    worker_id UUID,
    claim_id UUID,
    lease_until TIMESTAMPTZ,
    last_error TEXT CHECK (last_error IS NULL OR octet_length(last_error) <= 1000),
    result JSONB NOT NULL DEFAULT '{}'::jsonb,
    version BIGINT NOT NULL DEFAULT 1 CHECK (version > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((state = 'claimed') = (worker_id IS NOT NULL AND claim_id IS NOT NULL AND lease_until IS NOT NULL)),
    UNIQUE (run_id, work_kind, scope_key, request_hash)
);

CREATE INDEX IF NOT EXISTS migration_work_items_due_idx
    ON erpx_control.migration_work_items (available_at, work_id)
    WHERE state IN ('pending', 'retryable');

CREATE INDEX IF NOT EXISTS migration_work_items_lease_idx
    ON erpx_control.migration_work_items (lease_until, work_id)
    WHERE state = 'claimed';

CREATE TABLE IF NOT EXISTS erpx_control.migration_checkpoints (
    run_id UUID NOT NULL REFERENCES erpx_control.migration_runs(run_id) ON DELETE RESTRICT,
    work_kind TEXT NOT NULL,
    scope_key TEXT NOT NULL,
    source_contract_hash TEXT CHECK (source_contract_hash IS NULL OR source_contract_hash ~ '^[0-9a-fA-F]{64}$'),
    source_cursor TEXT,
    source_version BIGINT NOT NULL DEFAULT 0 CHECK (source_version >= 0),
    rows_seen BIGINT NOT NULL DEFAULT 0 CHECK (rows_seen >= 0),
    rows_applied BIGINT NOT NULL DEFAULT 0 CHECK (rows_applied >= 0),
    rows_replayed BIGINT NOT NULL DEFAULT 0 CHECK (rows_replayed >= 0),
    source_digest TEXT CHECK (source_digest IS NULL OR source_digest ~ '^[0-9a-fA-F]{64}$'),
    state TEXT NOT NULL CHECK (state IN ('pending', 'running', 'succeeded', 'failed', 'rolled_back')),
    checkpoint_version BIGINT NOT NULL DEFAULT 1 CHECK (checkpoint_version > 0),
    idempotency_key TEXT CHECK (idempotency_key IS NULL OR length(trim(idempotency_key)) BETWEEN 1 AND 200),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (run_id, work_kind, scope_key)
);

CREATE TABLE IF NOT EXISTS erpx_control.migration_reconciliation_reports (
    report_id UUID PRIMARY KEY,
    run_id UUID NOT NULL REFERENCES erpx_control.migration_runs(run_id) ON DELETE RESTRICT,
    context_key TEXT NOT NULL CHECK (length(trim(context_key)) BETWEEN 1 AND 200),
    source_counts JSONB NOT NULL,
    target_counts JSONB NOT NULL,
    source_totals JSONB NOT NULL,
    target_totals JSONB NOT NULL,
    differences JSONB NOT NULL,
    unexplained JSONB NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('matched', 'different', 'unexplained', 'unavailable')),
    evidence_hash TEXT NOT NULL CHECK (evidence_hash ~ '^[0-9a-fA-F]{64}$'),
    actor_ref TEXT NOT NULL CHECK (length(trim(actor_ref)) BETWEEN 1 AND 200),
    correlation_id UUID NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (run_id, context_key, evidence_hash)
);

CREATE INDEX IF NOT EXISTS migration_reconciliation_reports_run_idx
    ON erpx_control.migration_reconciliation_reports (run_id, context_key, recorded_at DESC);

CREATE TABLE IF NOT EXISTS erpx_control.migration_rollback_rehearsals (
    rehearsal_id UUID PRIMARY KEY,
    run_id UUID NOT NULL REFERENCES erpx_control.migration_runs(run_id) ON DELETE RESTRICT,
    target_snapshot_ref TEXT NOT NULL CHECK (length(trim(target_snapshot_ref)) BETWEEN 1 AND 200),
    pre_rollback_digest TEXT NOT NULL CHECK (pre_rollback_digest ~ '^[0-9a-fA-F]{64}$'),
    restored_digest TEXT CHECK (restored_digest IS NULL OR restored_digest ~ '^[0-9a-fA-F]{64}$'),
    state TEXT NOT NULL CHECK (state IN ('started', 'succeeded', 'failed')),
    source_preserved BOOLEAN NOT NULL DEFAULT FALSE,
    read_model_rebuilt BOOLEAN NOT NULL DEFAULT FALSE,
    error_present BOOLEAN NOT NULL DEFAULT FALSE,
    actor_ref TEXT NOT NULL CHECK (length(trim(actor_ref)) BETWEEN 1 AND 200),
    correlation_id UUID NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (run_id, target_snapshot_ref, pre_rollback_digest)
);

CREATE INDEX IF NOT EXISTS migration_rollback_rehearsals_run_idx
    ON erpx_control.migration_rollback_rehearsals (run_id, recorded_at DESC);


-- Durable, payload-bounded source snapshot evidence for the Legacy bridge.
-- This records a read-only source extraction and queues work; it never
-- changes Legacy or ERPX command ownership.

CREATE TABLE IF NOT EXISTS erpx_control.migration_source_snapshots (
    snapshot_id UUID PRIMARY KEY,
    run_id UUID NOT NULL REFERENCES erpx_control.migration_runs(run_id) ON DELETE RESTRICT,
    source_contract_hash TEXT NOT NULL CHECK (source_contract_hash ~ '^[0-9a-fA-F]{64}$'),
    extractor_version TEXT NOT NULL CHECK (length(trim(extractor_version)) BETWEEN 1 AND 128),
    snapshot JSONB NOT NULL,
    missing_table_count BIGINT NOT NULL CHECK (missing_table_count >= 0),
    entity_kind_count BIGINT NOT NULL CHECK (entity_kind_count >= 0),
    total_rows BIGINT NOT NULL CHECK (total_rows >= 0),
    captured_at TIMESTAMPTZ NOT NULL,
    actor_ref TEXT NOT NULL CHECK (length(trim(actor_ref)) BETWEEN 1 AND 200),
    correlation_id UUID NOT NULL,
    idempotency_key TEXT NOT NULL CHECK (length(trim(idempotency_key)) BETWEEN 1 AND 200),
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (octet_length(snapshot::text) <= 16777216),
    UNIQUE (run_id, source_contract_hash),
    UNIQUE (run_id, idempotency_key)
);

CREATE INDEX IF NOT EXISTS migration_source_snapshots_run_idx
    ON erpx_control.migration_source_snapshots (run_id, recorded_at DESC, snapshot_id);


-- Bind support SLA delivery acknowledgements to the exact worker lease.
-- Existing pending/delivered rows remain valid; newly claimed rows carry both
-- the worker identity and a unique durable claim token.
ALTER TABLE erpx_control.support_sla_notifications
    ADD COLUMN IF NOT EXISTS claimed_by UUID,
    ADD COLUMN IF NOT EXISTS claim_token UUID;

CREATE INDEX IF NOT EXISTS support_sla_notifications_claim_idx
    ON erpx_control.support_sla_notifications (notification_key, claimed_by, claim_token);


-- Preserve bounded, secret-free worker failure context beside the legacy
-- display error. The context is diagnostic state for the outbox row; it is
-- never an event payload and never a source-of-truth business mutation.
ALTER TABLE erpx_control.outbox_events
    ADD COLUMN IF NOT EXISTS last_error_context JSONB;

ALTER TABLE erpx_control.outbox_events
    DROP CONSTRAINT IF EXISTS outbox_last_error_context_object;

ALTER TABLE erpx_control.outbox_events
    ADD CONSTRAINT outbox_last_error_context_object
    CHECK (
        last_error_context IS NULL
        OR jsonb_typeof(last_error_context) = 'object'
    );


-- Durable, secret-free physical PITR admission metadata.
--
-- The recovery point contains only validated timestamps/LSNs and opaque
-- references. WAL bytes and physical clusters remain outside the control
-- database and are handled by the credentialed infrastructure executor.
ALTER TABLE erpx_control.holding_backup_jobs
    ADD COLUMN IF NOT EXISTS pitr_recovery_point JSONB;

ALTER TABLE erpx_control.holding_backup_jobs
    DROP CONSTRAINT IF EXISTS holding_backup_jobs_kind_check,
    DROP CONSTRAINT IF EXISTS holding_backup_jobs_check;

ALTER TABLE erpx_control.holding_backup_jobs
    ADD CONSTRAINT holding_backup_jobs_kind_shape_check CHECK (
        (kind = 'full'
            AND restore_target_ref IS NULL
            AND candidate_read_database_key IS NULL
            AND source_version IS NULL
            AND pitr_recovery_point IS NULL)
        OR (kind = 'restore_test'
            AND restore_target_ref IS NOT NULL
            AND candidate_read_database_key IS NOT NULL
            AND source_version > 0
            AND pitr_recovery_point IS NULL)
        OR (kind = 'pitr'
            AND restore_target_ref IS NULL
            AND candidate_read_database_key IS NULL
            AND source_version IS NULL
            AND jsonb_typeof(pitr_recovery_point) = 'object')
    );

ALTER TABLE erpx_control.holding_backup_runs
    ADD COLUMN IF NOT EXISTS pitr_recovery_point JSONB,
    ADD CONSTRAINT holding_backup_runs_pitr_recovery_point_shape_check CHECK (
        (kind = 'pitr' AND jsonb_typeof(pitr_recovery_point) = 'object')
        OR (kind <> 'pitr' AND pitr_recovery_point IS NULL)
    );


-- Durable control-plane scheduling for authorized Finance bank-provider imports.
-- Request/authorization JSON contains only validated opaque references and
-- authorization facts; credentials and provider payloads remain out of this
-- database. Holding facts stay in the routed holding write database.
CREATE TABLE IF NOT EXISTS erpx_control.finance_bank_provider_import_jobs (
    job_id UUID PRIMARY KEY,
    holding_id TEXT NOT NULL REFERENCES erpx_control.holding_registry(holding_id) ON DELETE RESTRICT,
    data_profile_key TEXT NOT NULL CHECK (length(trim(data_profile_key)) BETWEEN 1 AND 200),
    import_id UUID NOT NULL,
    provider_account_ref TEXT NOT NULL CHECK (length(trim(provider_account_ref)) BETWEEN 1 AND 255),
    provider_key TEXT NOT NULL CHECK (length(trim(provider_key)) BETWEEN 1 AND 64),
    credential_key_ref TEXT NOT NULL CHECK (length(trim(credential_key_ref)) BETWEEN 1 AND 255),
    source_fence BIGINT NOT NULL CHECK (source_fence > 0),
    request JSONB NOT NULL,
    authorization_facts JSONB NOT NULL,
    request_hash TEXT NOT NULL CHECK (length(request_hash) = 64),
    state TEXT NOT NULL CHECK (state IN ('pending', 'claimed', 'retryable', 'completed', 'dead_letter')),
    available_at TIMESTAMPTZ NOT NULL,
    max_attempts SMALLINT NOT NULL CHECK (max_attempts BETWEEN 1 AND 8),
    attempts SMALLINT NOT NULL DEFAULT 0 CHECK (attempts BETWEEN 0 AND 8),
    current_claim_id UUID,
    worker_id UUID,
    lease_until TIMESTAMPTZ,
    last_failure_class TEXT,
    dead_lettered_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((state = 'claimed') = (
        current_claim_id IS NOT NULL AND worker_id IS NOT NULL AND lease_until IS NOT NULL
    )),
    CHECK (last_failure_class IS NULL OR last_failure_class IN (
        'retryable', 'permanent', 'authorization', 'claim_lost', 'source_fence', 'reconciliation'
    )),
    UNIQUE (holding_id, data_profile_key, import_id)
);

CREATE INDEX IF NOT EXISTS finance_bank_provider_import_jobs_due_idx
    ON erpx_control.finance_bank_provider_import_jobs (available_at, job_id)
    WHERE state IN ('pending', 'retryable');

CREATE INDEX IF NOT EXISTS finance_bank_provider_import_jobs_claim_idx
    ON erpx_control.finance_bank_provider_import_jobs (lease_until, job_id)
    WHERE state = 'claimed';

CREATE TABLE IF NOT EXISTS erpx_control.finance_bank_provider_import_job_claims (
    claim_id UUID PRIMARY KEY,
    job_id UUID NOT NULL REFERENCES erpx_control.finance_bank_provider_import_jobs(job_id) ON DELETE RESTRICT,
    worker_id UUID NOT NULL,
    source_fence BIGINT NOT NULL CHECK (source_fence > 0),
    claimed_at TIMESTAMPTZ NOT NULL,
    lease_until TIMESTAMPTZ NOT NULL CHECK (lease_until > claimed_at),
    state TEXT NOT NULL CHECK (state IN ('active', 'completed', 'stale', 'failed', 'dead_letter')),
    completed_at TIMESTAMPTZ,
    outcome JSONB,
    UNIQUE (job_id, claim_id)
);

CREATE UNIQUE INDEX IF NOT EXISTS finance_bank_provider_import_active_claim_idx
    ON erpx_control.finance_bank_provider_import_job_claims (job_id)
    WHERE state = 'active';

CREATE INDEX IF NOT EXISTS finance_bank_provider_import_claim_worker_idx
    ON erpx_control.finance_bank_provider_import_job_claims (worker_id, state, lease_until);

CREATE TABLE IF NOT EXISTS erpx_control.finance_bank_provider_import_evidence (
    evidence_id UUID PRIMARY KEY,
    job_id UUID NOT NULL REFERENCES erpx_control.finance_bank_provider_import_jobs(job_id) ON DELETE RESTRICT,
    claim_id UUID NOT NULL REFERENCES erpx_control.finance_bank_provider_import_job_claims(claim_id) ON DELETE RESTRICT,
    evidence_kind TEXT NOT NULL CHECK (evidence_kind IN ('reconciliation', 'rollback')),
    payload JSONB NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS finance_bank_provider_import_evidence_job_idx
    ON erpx_control.finance_bank_provider_import_evidence (job_id, recorded_at, evidence_id);


-- Defense-in-depth for the durable holding projection scheduler.
--
-- Scheduler rows are control-plane coordination state, but their holding and
-- holding fields must remain inseparable even for recovery tooling or direct
-- SQL writers. The repository still performs active-state authorization and
-- locks the registry row; these composite references prevent a valid holding
-- paired with a different holding, job, or claim batch from being persisted.

CREATE UNIQUE INDEX IF NOT EXISTS holding_projection_jobs_id_scope_uq
    ON erpx_control.holding_projection_jobs (id, holding_id, data_profile_key);

CREATE UNIQUE INDEX IF NOT EXISTS holding_projection_claim_batches_binding_uq
    ON erpx_control.holding_projection_claim_batches
       (claim_key, holding_id, data_profile_key, worker_id);

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'erpx_control.holding_projection_jobs'::regclass
          AND conname = 'holding_projection_jobs_holding_holding_fk'
    ) THEN
        ALTER TABLE erpx_control.holding_projection_jobs
            ADD CONSTRAINT holding_projection_jobs_holding_holding_fk
            FOREIGN KEY (holding_id, data_profile_key)
            REFERENCES erpx_control.holding_registry (holding_id, data_profile_key)
            ON DELETE RESTRICT;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'erpx_control.holding_projection_claim_batches'::regclass
          AND conname = 'holding_projection_claim_batches_holding_holding_fk'
    ) THEN
        ALTER TABLE erpx_control.holding_projection_claim_batches
            ADD CONSTRAINT holding_projection_claim_batches_holding_holding_fk
            FOREIGN KEY (holding_id, data_profile_key)
            REFERENCES erpx_control.holding_registry (holding_id, data_profile_key)
            ON DELETE RESTRICT;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'erpx_control.holding_projection_claims'::regclass
          AND conname = 'holding_projection_claims_holding_holding_fk'
    ) THEN
        ALTER TABLE erpx_control.holding_projection_claims
            ADD CONSTRAINT holding_projection_claims_holding_holding_fk
            FOREIGN KEY (holding_id, data_profile_key)
            REFERENCES erpx_control.holding_registry (holding_id, data_profile_key)
            ON DELETE RESTRICT;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'erpx_control.holding_projection_claims'::regclass
          AND conname = 'holding_projection_claims_job_binding_fk'
    ) THEN
        ALTER TABLE erpx_control.holding_projection_claims
            ADD CONSTRAINT holding_projection_claims_job_binding_fk
            FOREIGN KEY (job_id, holding_id, data_profile_key)
            REFERENCES erpx_control.holding_projection_jobs (id, holding_id, data_profile_key)
            ON DELETE RESTRICT;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'erpx_control.holding_projection_claims'::regclass
          AND conname = 'holding_projection_claims_batch_binding_fk'
    ) THEN
        ALTER TABLE erpx_control.holding_projection_claims
            ADD CONSTRAINT holding_projection_claims_batch_binding_fk
            FOREIGN KEY (claim_key, holding_id, data_profile_key, worker_id)
            REFERENCES erpx_control.holding_projection_claim_batches
                (claim_key, holding_id, data_profile_key, worker_id)
            ON DELETE RESTRICT;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'erpx_control.holding_projection_rebuilds'::regclass
          AND conname = 'holding_projection_rebuilds_holding_holding_fk'
    ) THEN
        ALTER TABLE erpx_control.holding_projection_rebuilds
            ADD CONSTRAINT holding_projection_rebuilds_holding_holding_fk
            FOREIGN KEY (holding_id, data_profile_key)
            REFERENCES erpx_control.holding_registry (holding_id, data_profile_key)
            ON DELETE RESTRICT;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'erpx_control.holding_projection_rebuilds'::regclass
          AND conname = 'holding_projection_rebuilds_job_binding_fk'
    ) THEN
        ALTER TABLE erpx_control.holding_projection_rebuilds
            ADD CONSTRAINT holding_projection_rebuilds_job_binding_fk
            FOREIGN KEY (job_id, holding_id, data_profile_key)
            REFERENCES erpx_control.holding_projection_jobs (id, holding_id, data_profile_key)
            ON DELETE RESTRICT;
    END IF;
END
$$;


-- Every queued G13 work item must carry a content-addressed source fence.
-- Repository methods additionally require equality with the immutable run
-- contract hash; this constraint protects direct SQL and recovery writers.

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'erpx_control.migration_work_items'::regclass
          AND conname = 'migration_work_items_source_fence_sha256_ck'
    ) THEN
        ALTER TABLE erpx_control.migration_work_items
            ADD CONSTRAINT migration_work_items_source_fence_sha256_ck
            CHECK (source_fence ~ '^[0-9a-fA-F]{64}$');
    END IF;
END
$$;


-- ERPX development cutover authority.
--
-- The singleton is intentionally immutable: this repository no longer
-- contains a legacy command bridge, so command ownership cannot be changed by
-- configuration or an environment fallback. Any future ownership model needs
-- an explicit forward migration with its own audit evidence.
CREATE TABLE IF NOT EXISTS erpx_control.command_ownership (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    mode TEXT NOT NULL CHECK (mode = 'erpx_only'),
    evidence_id UUID NOT NULL UNIQUE,
    established_at TIMESTAMPTZ NOT NULL,
    established_by TEXT NOT NULL CHECK (btrim(established_by) <> '')
);

CREATE TABLE IF NOT EXISTS erpx_control.command_ownership_events (
    event_id UUID PRIMARY KEY,
    mode TEXT NOT NULL CHECK (mode = 'erpx_only'),
    actor_ref TEXT NOT NULL CHECK (btrim(actor_ref) <> ''),
    correlation_id UUID NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    evidence JSONB NOT NULL
);

CREATE OR REPLACE FUNCTION erpx_control.reject_ownership_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'ERPX command ownership evidence is append-only'
        USING ERRCODE = '55000';
END;
$$;

DROP TRIGGER IF EXISTS command_ownership_immutable
    ON erpx_control.command_ownership;
CREATE TRIGGER command_ownership_immutable
BEFORE UPDATE OR DELETE ON erpx_control.command_ownership
FOR EACH ROW EXECUTE FUNCTION erpx_control.reject_ownership_mutation();

DROP TRIGGER IF EXISTS command_ownership_events_append_only
    ON erpx_control.command_ownership_events;
CREATE TRIGGER command_ownership_events_append_only
BEFORE UPDATE OR DELETE ON erpx_control.command_ownership_events
FOR EACH ROW EXECUTE FUNCTION erpx_control.reject_ownership_mutation();

CREATE OR REPLACE FUNCTION erpx_control.require_erpx_command_owner()
RETURNS void
LANGUAGE plpgsql
STABLE
AS $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM erpx_control.command_ownership
        WHERE singleton AND mode = 'erpx_only'
    ) THEN
        RAISE EXCEPTION 'authoritative ERPX command ownership is unavailable'
            USING ERRCODE = '55000';
    END IF;
END;
$$;

INSERT INTO erpx_control.command_ownership
    (singleton, mode, evidence_id, established_at, established_by)
VALUES (
    TRUE,
    'erpx_only',
    '4e1a4e86-f821-5f73-84e7-04f30bcc055c',
    now(),
    'cutover:migration-0066'
)
ON CONFLICT (singleton) DO NOTHING;

INSERT INTO erpx_control.command_ownership_events
    (event_id, mode, actor_ref, correlation_id, occurred_at, evidence)
VALUES (
    '4e1a4e86-f821-5f73-84e7-04f30bcc055c',
    'erpx_only',
    'cutover:migration-0066',
    '86da8dba-644d-5f9a-90b7-8d02ab39e6db',
    now(),
    '{"event":"erpx.command_ownership.established","rollback":"intentionally_absent"}'::jsonb
)
ON CONFLICT (event_id) DO NOTHING;

INSERT INTO erpx_control.audit_events
    (id, actor_ref, customer_account_id, action, purpose, correlation_id, payload)
VALUES (
    '7d1c7d6f-1786-5a52-aa06-4bbce4261f61',
    'cutover:migration-0066',
    NULL,
    'platform.command_ownership.establish',
    'Persist ERPX-only development command authority',
    '86da8dba-644d-5f9a-90b7-8d02ab39e6db',
    '{"mode":"erpx_only","append_only":true}'::jsonb
)
ON CONFLICT (id) DO NOTHING;

INSERT INTO erpx_control.outbox_events (
    id, event_type, event_version, aggregate_type, aggregate_id,
    aggregate_version, active_scope, actor, correlation_id, causation_id,
    idempotency_key, payload, occurred_at
)
VALUES (
    '8a84e11c-0593-5a2c-b86a-9e07a6b1ca56',
    'erpx.command_ownership.established',
    1,
    'command_ownership',
    '4e1a4e86-f821-5f73-84e7-04f30bcc055c',
    1,
    NULL,
    '{"kind":"system","subject":"cutover:migration-0066"}'::jsonb,
    '86da8dba-644d-5f9a-90b7-8d02ab39e6db',
    NULL,
    'cutover:erpx-only:v1',
    '{"mode":"erpx_only","rollback":"intentionally_absent"}'::jsonb,
    now()
)
ON CONFLICT (id) DO NOTHING;

-- Idempotent development fixture receipts. The fixture command writes one
-- bounded row per context and one outbox event per successful command.
CREATE TABLE IF NOT EXISTS erpx_control.demo_seed_commands (
    idempotency_key TEXT PRIMARY KEY CHECK (btrim(idempotency_key) <> ''),
    fixture_version TEXT NOT NULL CHECK (btrim(fixture_version) <> ''),
    content_sha256 TEXT NOT NULL CHECK (length(content_sha256) = 64),
    completed_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS erpx_control.demo_contexts (
    context_key TEXT PRIMARY KEY CHECK (btrim(context_key) <> ''),
    display_name TEXT NOT NULL CHECK (btrim(display_name) <> ''),
    holding_id TEXT NOT NULL,
    data_profile_key TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('ready', 'unconfigured', 'disabled')),
    sample_count BIGINT NOT NULL CHECK (sample_count >= 0),
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    updated_at TIMESTAMPTZ NOT NULL
);


-- Cluster-scoped physical PITR. Holding backup jobs remain logical-only;
-- physical recovery coordinates and object keys belong to one PostgreSQL
-- cluster chain and never to a holding namespace.

CREATE OR REPLACE FUNCTION erpx_control.reject_holding_pitr_job()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.kind = 'pitr' THEN
        RAISE EXCEPTION 'holding PITR is unsupported; use cluster-scoped PITR';
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS holding_backup_jobs_reject_pitr
    ON erpx_control.holding_backup_jobs;
CREATE TRIGGER holding_backup_jobs_reject_pitr
    BEFORE INSERT OR UPDATE OF kind ON erpx_control.holding_backup_jobs
    FOR EACH ROW
    EXECUTE FUNCTION erpx_control.reject_holding_pitr_job();

CREATE TABLE IF NOT EXISTS erpx_control.platform_pitr_clusters (
    cluster_ref TEXT PRIMARY KEY CHECK (length(trim(cluster_ref)) BETWEEN 1 AND 120),
    storage_namespace TEXT NOT NULL CHECK (length(trim(storage_namespace)) BETWEEN 3 AND 63),
    state TEXT NOT NULL CHECK (state IN ('configured', 'active', 'suspended', 'failed')),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS erpx_control.platform_pitr_chains (
    id UUID PRIMARY KEY,
    cluster_ref TEXT NOT NULL REFERENCES erpx_control.platform_pitr_clusters(cluster_ref),
    chain_ref TEXT NOT NULL CHECK (length(trim(chain_ref)) BETWEEN 1 AND 120),
    state TEXT NOT NULL CHECK (
        state IN ('scheduled', 'capturing', 'continuous', 'degraded', 'expired', 'failed')
    ),
    base_backup_object_key TEXT,
    base_backup_sha256 TEXT CHECK (base_backup_sha256 IS NULL OR length(base_backup_sha256) = 64),
    start_lsn TEXT,
    stop_lsn TEXT,
    timeline_id BIGINT CHECK (timeline_id IS NULL OR timeline_id > 0),
    retention_until TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE (cluster_ref, chain_ref),
    CHECK (retention_until > created_at),
    CHECK (
        base_backup_object_key IS NULL
        OR base_backup_object_key LIKE 'platform/%/chains/%/base/%'
    )
);

CREATE INDEX IF NOT EXISTS platform_pitr_chains_retention_idx
    ON erpx_control.platform_pitr_chains (state, retention_until, id);

CREATE TABLE IF NOT EXISTS erpx_control.platform_pitr_wal_segments (
    chain_id UUID NOT NULL REFERENCES erpx_control.platform_pitr_chains(id),
    segment_name TEXT NOT NULL CHECK (segment_name ~ '^[0-9A-F]{24}([.][a-z0-9]+)?$'),
    object_key TEXT NOT NULL,
    size_bytes BIGINT NOT NULL CHECK (size_bytes > 0),
    sha256 TEXT NOT NULL CHECK (length(sha256) = 64),
    archived_at TIMESTAMPTZ NOT NULL,
    verified_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (chain_id, segment_name),
    UNIQUE (object_key),
    CHECK (verified_at >= archived_at),
    CHECK (object_key LIKE 'platform/%/chains/%/wal/%')
);

CREATE TABLE IF NOT EXISTS erpx_control.platform_pitr_recovery_points (
    id UUID PRIMARY KEY,
    chain_id UUID NOT NULL REFERENCES erpx_control.platform_pitr_chains(id),
    label TEXT NOT NULL CHECK (length(trim(label)) BETWEEN 1 AND 120),
    target_time TIMESTAMPTZ,
    target_lsn TEXT,
    timeline_id BIGINT NOT NULL CHECK (timeline_id > 0),
    before_marker TEXT,
    after_marker TEXT,
    wal_continuity_verified BOOLEAN NOT NULL DEFAULT false,
    state TEXT NOT NULL CHECK (state IN ('available', 'blocked', 'expired')),
    created_at TIMESTAMPTZ NOT NULL,
    CHECK ((target_time IS NULL) <> (target_lsn IS NULL)),
    UNIQUE (chain_id, label)
);

CREATE INDEX IF NOT EXISTS platform_pitr_recovery_points_chain_idx
    ON erpx_control.platform_pitr_recovery_points (chain_id, created_at DESC, id DESC);

CREATE TABLE IF NOT EXISTS erpx_control.platform_pitr_restore_rehearsals (
    id UUID PRIMARY KEY,
    recovery_point_id UUID NOT NULL REFERENCES erpx_control.platform_pitr_recovery_points(id),
    restore_target_ref TEXT NOT NULL CHECK (length(trim(restore_target_ref)) BETWEEN 1 AND 120),
    state TEXT NOT NULL CHECK (
        state IN ('pending', 'claimed', 'restoring', 'verifying', 'completed', 'failed', 'cleaned')
    ),
    worker_id UUID,
    lease_until TIMESTAMPTZ,
    verification_summary JSONB NOT NULL DEFAULT '{}'::jsonb CHECK (
        jsonb_typeof(verification_summary) = 'object'
    ),
    failure_category TEXT,
    requested_by TEXT NOT NULL,
    correlation_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (
        (state = 'claimed' AND worker_id IS NOT NULL AND lease_until IS NOT NULL)
        OR state <> 'claimed'
    )
);

CREATE INDEX IF NOT EXISTS platform_pitr_restore_rehearsals_due_idx
    ON erpx_control.platform_pitr_restore_rehearsals (state, created_at, id)
    WHERE state IN ('pending', 'claimed', 'restoring', 'verifying');

CREATE TABLE IF NOT EXISTS erpx_control.platform_pitr_promotion_audit (
    id UUID PRIMARY KEY,
    rehearsal_id UUID NOT NULL REFERENCES erpx_control.platform_pitr_restore_rehearsals(id),
    actor_ref TEXT NOT NULL,
    runtime_stopped BOOLEAN NOT NULL,
    confirmation_digest TEXT NOT NULL CHECK (length(confirmation_digest) = 64),
    outcome TEXT NOT NULL CHECK (outcome IN ('accepted', 'rejected')),
    occurred_at TIMESTAMPTZ NOT NULL
);


-- Durable, payload-free recovery markers used to prove that a physical
-- cluster restore stopped at the requested WAL/time boundary. These rows are
-- platform-scoped; no holding may create or interpret a physical target.

CREATE TABLE IF NOT EXISTS erpx_control.platform_pitr_recovery_markers (
    id UUID PRIMARY KEY,
    rehearsal_ref TEXT NOT NULL CHECK (length(trim(rehearsal_ref)) BETWEEN 1 AND 120),
    marker_kind TEXT NOT NULL CHECK (marker_kind IN ('before', 'after')),
    committed_at TIMESTAMPTZ NOT NULL,
    UNIQUE (rehearsal_ref, marker_kind)
);



-- Three-layer SaaS control-plane bindings and short-lived entered-holding
-- administration. This migration is additive: it performs no ownership
-- guessing, holding activation, billing transition, suspension, or deletion.

CREATE TABLE IF NOT EXISTS erpx_control.customer_holding_bindings (
    customer_id UUID PRIMARY KEY
        REFERENCES erpx_control.customers(id) ON DELETE RESTRICT,
    holding_id TEXT NOT NULL UNIQUE
        REFERENCES erpx_control.holding_registry(holding_id) ON DELETE RESTRICT,
    canonical_holding_id UUID NOT NULL UNIQUE,
    data_profile_key TEXT NOT NULL,
    binding_state TEXT NOT NULL DEFAULT 'configured'
        CHECK (binding_state IN ('configured', 'configuration_required')),
    bound_by TEXT NOT NULL,
    bound_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (length(btrim(holding_id)) BETWEEN 1 AND 128),
    CHECK (length(btrim(data_profile_key)) BETWEEN 1 AND 255),
    CHECK (length(btrim(bound_by)) BETWEEN 1 AND 255),
    UNIQUE (customer_id, holding_id),
    UNIQUE (customer_id, holding_id, canonical_holding_id)
);

-- The client receives a random opaque UUID. Only its SHA-256 digest is stored,
-- so database reads cannot recover an active bearer value.
CREATE TABLE IF NOT EXISTS erpx_control.platform_holding_admin_contexts (
    context_id_hash BYTEA PRIMARY KEY CHECK (octet_length(context_id_hash) = 32),
    principal_ref TEXT NOT NULL,
    device_id UUID NOT NULL,
    session_id UUID NOT NULL,
    holding_id TEXT NOT NULL
        REFERENCES erpx_control.customer_holding_bindings(holding_id) ON DELETE RESTRICT,
    customer_id UUID NOT NULL,
    canonical_holding_id UUID NOT NULL,
    reason TEXT NOT NULL,
    correlation_id UUID NOT NULL,
    issued_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    revocation_reason TEXT,
    CHECK (expires_at = issued_at + INTERVAL '30 minutes'),
    CHECK (length(btrim(principal_ref)) BETWEEN 1 AND 255),
    CHECK (length(btrim(reason)) BETWEEN 1 AND 512),
    CHECK ((revoked_at IS NULL) = (revocation_reason IS NULL)),
    FOREIGN KEY (customer_id, holding_id, canonical_holding_id)
        REFERENCES erpx_control.customer_holding_bindings
            (customer_id, holding_id, canonical_holding_id)
        ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS platform_holding_admin_context_active_idx
    ON erpx_control.platform_holding_admin_contexts
        (principal_ref, device_id, session_id, holding_id, expires_at)
    WHERE revoked_at IS NULL;

CREATE TABLE IF NOT EXISTS erpx_control.stripe_subscription_bindings (
    customer_id UUID NOT NULL,
    holding_id TEXT NOT NULL,
    subscription_id UUID NOT NULL,
    stripe_customer_ref TEXT NOT NULL UNIQUE,
    stripe_subscription_ref TEXT UNIQUE,
    currency CHAR(3) NOT NULL,
    item_quantities JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (customer_id, subscription_id),
    FOREIGN KEY (customer_id, holding_id)
        REFERENCES erpx_control.customer_holding_bindings (customer_id, holding_id)
        ON DELETE RESTRICT,
    CHECK (currency ~ '^[A-Z]{3}$'),
    CHECK (jsonb_typeof(item_quantities) = 'object'),
    CHECK (length(btrim(stripe_customer_ref)) BETWEEN 1 AND 255),
    CHECK (stripe_subscription_ref IS NULL OR length(btrim(stripe_subscription_ref)) BETWEEN 1 AND 255)
);

-- Checkout resolves these private provider identifiers from the published
-- catalog revision. They are never accepted from, or returned to, a UI.
CREATE TABLE IF NOT EXISTS erpx_control.stripe_catalog_price_bindings (
    catalog_revision INTEGER NOT NULL
        REFERENCES erpx_control.commercial_catalog_revisions(revision_number)
        ON DELETE RESTRICT,
    item_key TEXT NOT NULL,
    stripe_product_ref TEXT NOT NULL,
    stripe_price_ref TEXT NOT NULL UNIQUE,
    currency CHAR(3) NOT NULL,
    active BOOLEAN NOT NULL DEFAULT TRUE,
    bound_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (catalog_revision, item_key),
    CHECK (catalog_revision > 0),
    CHECK (length(btrim(item_key)) BETWEEN 1 AND 255),
    CHECK (stripe_product_ref ~ '^prod_[A-Za-z0-9_]+$'),
    CHECK (stripe_price_ref ~ '^price_[A-Za-z0-9_]+$'),
    CHECK (currency ~ '^[A-Z]{3}$')
);

CREATE TABLE IF NOT EXISTS erpx_control.billing_grace_extensions (
    id UUID PRIMARY KEY,
    customer_id UUID NOT NULL,
    subscription_id UUID NOT NULL,
    delinquency_cycle_id UUID NOT NULL,
    previous_grace_until TIMESTAMPTZ NOT NULL,
    extended_until TIMESTAMPTZ NOT NULL,
    granted_at TIMESTAMPTZ NOT NULL,
    granted_by TEXT NOT NULL,
    reason TEXT NOT NULL,
    correlation_id UUID NOT NULL,
    UNIQUE (customer_id, subscription_id, delinquency_cycle_id),
    FOREIGN KEY (customer_id, subscription_id)
        REFERENCES erpx_control.billing_lifecycles (customer_id, subscription_id)
        ON DELETE RESTRICT,
    CHECK (extended_until > previous_grace_until),
    CHECK (extended_until <= previous_grace_until + INTERVAL '7 days'),
    CHECK (length(btrim(granted_by)) BETWEEN 1 AND 255),
    CHECK (length(btrim(reason)) BETWEEN 1 AND 512)
);

CREATE TABLE IF NOT EXISTS erpx_control.holding_platform_access_audit (
    id UUID PRIMARY KEY,
    holding_id TEXT NOT NULL,
    customer_id UUID NOT NULL,
    canonical_holding_id UUID NOT NULL,
    company_id UUID,
    branch_id UUID,
    platform_actor_ref TEXT NOT NULL,
    module_key TEXT NOT NULL,
    action TEXT NOT NULL,
    reason TEXT NOT NULL,
    administrative_context_hash BYTEA NOT NULL
        CHECK (octet_length(administrative_context_hash) = 32),
    correlation_id UUID NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    FOREIGN KEY (customer_id, holding_id, canonical_holding_id)
        REFERENCES erpx_control.customer_holding_bindings
            (customer_id, holding_id, canonical_holding_id)
        ON DELETE RESTRICT,
    CHECK (branch_id IS NULL OR company_id IS NOT NULL),
    CHECK (length(btrim(platform_actor_ref)) BETWEEN 1 AND 255),
    CHECK (length(btrim(module_key)) BETWEEN 1 AND 128),
    CHECK (length(btrim(action)) BETWEEN 1 AND 255),
    CHECK (length(btrim(reason)) BETWEEN 1 AND 512)
);

CREATE INDEX IF NOT EXISTS holding_platform_access_audit_holding_idx
    ON erpx_control.holding_platform_access_audit
        (holding_id, occurred_at DESC, id DESC);

-- The all-holding registry uses a disposable control-plane projection. It holds
-- bounded operational/commercial summaries only and contains no holding
-- business records or holding-database routing secrets.
CREATE TABLE IF NOT EXISTS erpx_control.holding_summary_projection (
    holding_id TEXT PRIMARY KEY,
    customer_id UUID,
    canonical_holding_id UUID,
    customer_name TEXT NOT NULL DEFAULT '',
    holding_state TEXT NOT NULL,
    configuration_state TEXT NOT NULL
        CHECK (configuration_state IN ('configured', 'configuration_required')),
    subscription_state TEXT NOT NULL DEFAULT 'unconfigured',
    plan_key TEXT NOT NULL DEFAULT '',
    enabled_modules TEXT[] NOT NULL DEFAULT '{}',
    purchased_seats INTEGER NOT NULL DEFAULT 0 CHECK (purchased_seats >= 0),
    used_seats INTEGER NOT NULL DEFAULT 0 CHECK (used_seats >= 0),
    company_count INTEGER NOT NULL DEFAULT 0 CHECK (company_count >= 0),
    branch_count INTEGER NOT NULL DEFAULT 0 CHECK (branch_count >= 0),
    renewal_at TIMESTAMPTZ,
    overdue BOOLEAN NOT NULL DEFAULT FALSE,
    projection_freshness TEXT NOT NULL DEFAULT 'unknown',
    storage_state TEXT NOT NULL DEFAULT 'unknown',
    backup_state TEXT NOT NULL DEFAULT 'unknown',
    integration_health TEXT NOT NULL DEFAULT 'unknown',
    health TEXT NOT NULL DEFAULT 'unknown',
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (used_seats <= purchased_seats OR configuration_state = 'configuration_required')
);

INSERT INTO erpx_control.holding_summary_projection
    (holding_id, holding_state, configuration_state)
SELECT registry.holding_id, registry.state,
       CASE WHEN binding.customer_id IS NULL THEN 'configuration_required' ELSE 'configured' END
FROM erpx_control.holding_registry AS registry
LEFT JOIN erpx_control.customer_holding_bindings AS binding
  ON binding.holding_id = registry.holding_id
ON CONFLICT (holding_id) DO NOTHING;

CREATE OR REPLACE FUNCTION erpx_control.refresh_holding_summary_identity()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    selected_holding_id TEXT;
BEGIN
    selected_holding_id := COALESCE(NEW.holding_id, OLD.holding_id);
    INSERT INTO erpx_control.holding_summary_projection
        (holding_id, customer_id, canonical_holding_id, customer_name,
         holding_state, configuration_state, projected_at)
    SELECT registry.holding_id,
           binding.customer_id,
           binding.canonical_holding_id,
           COALESCE(customer.trading_name, customer.legal_name, ''),
           registry.state,
           CASE WHEN binding.customer_id IS NULL
                THEN 'configuration_required' ELSE 'configured' END,
           now()
    FROM erpx_control.holding_registry AS registry
    LEFT JOIN erpx_control.customer_holding_bindings AS binding
      ON binding.holding_id = registry.holding_id
    LEFT JOIN erpx_control.customers AS customer
      ON customer.id = binding.customer_id
    WHERE registry.holding_id = selected_holding_id
    ON CONFLICT (holding_id) DO UPDATE
    SET customer_id = EXCLUDED.customer_id,
        canonical_holding_id = EXCLUDED.canonical_holding_id,
        customer_name = EXCLUDED.customer_name,
        holding_state = EXCLUDED.holding_state,
        configuration_state = EXCLUDED.configuration_state,
        projected_at = EXCLUDED.projected_at;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS holding_registry_summary_refresh
    ON erpx_control.holding_registry;
CREATE TRIGGER holding_registry_summary_refresh
AFTER INSERT OR UPDATE OF state ON erpx_control.holding_registry
FOR EACH ROW EXECUTE FUNCTION erpx_control.refresh_holding_summary_identity();

DROP TRIGGER IF EXISTS customer_holding_binding_summary_refresh
    ON erpx_control.customer_holding_bindings;
CREATE TRIGGER customer_holding_binding_summary_refresh
AFTER INSERT OR UPDATE ON erpx_control.customer_holding_bindings
FOR EACH ROW EXECUTE FUNCTION erpx_control.refresh_holding_summary_identity();

CREATE INDEX IF NOT EXISTS holding_summary_projection_filter_idx
    ON erpx_control.holding_summary_projection
        (configuration_state, holding_state, subscription_state, health, holding_id);

CREATE INDEX IF NOT EXISTS holding_summary_projection_search_idx
    ON erpx_control.holding_summary_projection
        USING GIN (to_tsvector('simple', customer_name || ' ' || holding_id));

CREATE TABLE IF NOT EXISTS erpx_control.holding_invoice_summary_projection (
    holding_id TEXT NOT NULL,
    invoice_id TEXT NOT NULL,
    state TEXT NOT NULL,
    currency CHAR(3) NOT NULL,
    amount_due_minor BIGINT NOT NULL CHECK (amount_due_minor >= 0),
    amount_paid_minor BIGINT NOT NULL CHECK (amount_paid_minor >= 0),
    due_at TIMESTAMPTZ,
    hosted_invoice_available BOOLEAN NOT NULL DEFAULT FALSE,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_id, invoice_id),
    CHECK (currency ~ '^[A-Z]{3}$')
);

CREATE INDEX IF NOT EXISTS holding_invoice_summary_projection_page_idx
    ON erpx_control.holding_invoice_summary_projection
        (holding_id, projected_at DESC, invoice_id DESC);


-- Customer-hosted data-plane control state. This migration is additive and
-- defaults only already-bound holdings to ERPX-managed execution. It stores
-- secret-manager references, hashes, fingerprints, and redacted observations;
-- raw database, storage, directory, bootstrap, or capability secrets are not
-- part of this schema.

-- PostgreSQL requires an exact unique key for the composite holding/holding
-- foreign keys below. Migration 0069 makes both columns individually unique,
-- but that does not make the pair a valid referenced key.
ALTER TABLE erpx_control.customer_holding_bindings
    ADD CONSTRAINT customer_holding_bindings_holding_holding_unique
    UNIQUE (holding_id, canonical_holding_id);

CREATE TABLE IF NOT EXISTS erpx_control.holding_infrastructure_profiles (
    holding_id TEXT PRIMARY KEY
        REFERENCES erpx_control.customer_holding_bindings(holding_id) ON DELETE RESTRICT,
    canonical_holding_id UUID NOT NULL,
    deployment_mode TEXT NOT NULL
        CHECK (deployment_mode IN ('erpx_managed', 'customer_data_plane')),
    write_database_ref TEXT NOT NULL,
    read_database_ref TEXT NOT NULL,
    storage_ref TEXT NOT NULL,
    directory_credential_ref TEXT,
    backup_profile_ref TEXT NOT NULL,
    profile_version BIGINT NOT NULL CHECK (profile_version > 0),
    updated_by TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    FOREIGN KEY (holding_id, canonical_holding_id)
        REFERENCES erpx_control.customer_holding_bindings (holding_id, canonical_holding_id)
        ON DELETE RESTRICT,
    CHECK (write_database_ref <> read_database_ref),
    CHECK (length(btrim(write_database_ref)) BETWEEN 1 AND 255),
    CHECK (length(btrim(read_database_ref)) BETWEEN 1 AND 255),
    CHECK (length(btrim(storage_ref)) BETWEEN 1 AND 255),
    CHECK (directory_credential_ref IS NULL
        OR length(btrim(directory_credential_ref)) BETWEEN 1 AND 255),
    CHECK (length(btrim(backup_profile_ref)) BETWEEN 1 AND 255),
    CHECK (length(btrim(updated_by)) BETWEEN 1 AND 255),
    CHECK (
        (deployment_mode = 'erpx_managed'
            AND write_database_ref LIKE 'erpx-managed:%'
            AND read_database_ref LIKE 'erpx-managed:%'
            AND storage_ref LIKE 'erpx-managed:%'
            AND backup_profile_ref LIKE 'erpx-managed:%')
        OR
        (deployment_mode = 'customer_data_plane'
            AND write_database_ref LIKE 'customer-secret:%'
            AND read_database_ref LIKE 'customer-secret:%'
            AND storage_ref LIKE 'customer-secret:%'
            AND backup_profile_ref LIKE 'customer-secret:%')
    )
);

INSERT INTO erpx_control.holding_infrastructure_profiles
    (holding_id, canonical_holding_id, deployment_mode, write_database_ref,
     read_database_ref, storage_ref, backup_profile_ref, profile_version, updated_by)
SELECT binding.holding_id,
       binding.canonical_holding_id,
       'erpx_managed',
       'erpx-managed:registry/' || md5(binding.holding_id || ':write'),
       'erpx-managed:registry/' || md5(binding.holding_id || ':read'),
       'erpx-managed:registry/' || md5(binding.holding_id || ':storage'),
       'erpx-managed:registry/' || md5(binding.holding_id || ':backup'),
       1,
       'migration:0070'
FROM erpx_control.customer_holding_bindings AS binding
ON CONFLICT (holding_id) DO NOTHING;

CREATE TABLE IF NOT EXISTS erpx_control.data_plane_agent_enrollments (
    enrollment_id UUID PRIMARY KEY,
    holding_id TEXT NOT NULL,
    canonical_holding_id UUID NOT NULL,
    token_hash BYTEA NOT NULL UNIQUE CHECK (octet_length(token_hash) = 32),
    manifest_hash BYTEA NOT NULL CHECK (octet_length(manifest_hash) = 32),
    issued_by TEXT NOT NULL,
    issued_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ,
    consumed_by_agent_id UUID,
    revoked_at TIMESTAMPTZ,
    correlation_id UUID NOT NULL,
    FOREIGN KEY (holding_id, canonical_holding_id)
        REFERENCES erpx_control.customer_holding_bindings (holding_id, canonical_holding_id)
        ON DELETE RESTRICT,
    CHECK (expires_at > issued_at AND expires_at <= issued_at + INTERVAL '30 minutes'),
    CHECK (length(btrim(issued_by)) BETWEEN 1 AND 255),
    CHECK (consumed_at IS NULL OR consumed_by_agent_id IS NOT NULL),
    CHECK (NOT (consumed_at IS NOT NULL AND revoked_at IS NOT NULL))
);

CREATE INDEX IF NOT EXISTS data_plane_agent_enrollment_active_idx
    ON erpx_control.data_plane_agent_enrollments (holding_id, expires_at)
    WHERE consumed_at IS NULL AND revoked_at IS NULL;

CREATE TABLE IF NOT EXISTS erpx_control.data_plane_agents (
    agent_id UUID PRIMARY KEY,
    holding_id TEXT NOT NULL UNIQUE,
    canonical_holding_id UUID NOT NULL,
    certificate_fingerprint CHAR(64) NOT NULL UNIQUE
        CHECK (certificate_fingerprint ~ '^[0-9A-Fa-f]{64}$'),
    capabilities TEXT[] NOT NULL CHECK (cardinality(capabilities) > 0),
    state TEXT NOT NULL CHECK (state IN (
        'unenrolled', 'enrolling', 'connected', 'healthy', 'drifted',
        'degraded', 'offline', 'suspended', 'upgrade_required'
    )),
    observed_version BIGINT NOT NULL DEFAULT 0 CHECK (observed_version >= 0),
    agent_version TEXT NOT NULL,
    certificate_expires_at TIMESTAMPTZ NOT NULL,
    enrolled_at TIMESTAMPTZ NOT NULL,
    last_heartbeat_at TIMESTAMPTZ,
    suspended_at TIMESTAMPTZ,
    FOREIGN KEY (holding_id, canonical_holding_id)
        REFERENCES erpx_control.customer_holding_bindings (holding_id, canonical_holding_id)
        ON DELETE RESTRICT,
    CHECK (length(btrim(agent_version)) BETWEEN 1 AND 128),
    CHECK (certificate_expires_at > enrolled_at),
    CHECK ((state = 'suspended') = (suspended_at IS NOT NULL))
);

CREATE INDEX IF NOT EXISTS data_plane_agents_health_idx
    ON erpx_control.data_plane_agents (state, last_heartbeat_at, holding_id);

CREATE TABLE IF NOT EXISTS erpx_control.holding_desired_configurations (
    configuration_id UUID PRIMARY KEY,
    holding_id TEXT NOT NULL,
    canonical_holding_id UUID NOT NULL,
    target_agent_id UUID,
    deployment_mode TEXT NOT NULL
        CHECK (deployment_mode IN ('erpx_managed', 'customer_data_plane')),
    configuration_layer TEXT NOT NULL
        CHECK (configuration_layer IN ('platform', 'holding', 'business_unit')),
    business_unit TEXT,
    expected_observed_version BIGINT NOT NULL CHECK (expected_observed_version >= 0),
    desired_version BIGINT NOT NULL CHECK (desired_version > 0),
    lifecycle TEXT NOT NULL CHECK (lifecycle IN (
        'draft', 'awaiting_approval', 'approved', 'applying', 'verified',
        'active', 'rejected', 'drifted', 'failed', 'suspended'
    )),
    configuration_hash BYTEA NOT NULL CHECK (octet_length(configuration_hash) = 32),
    desired_state JSONB NOT NULL CHECK (jsonb_typeof(desired_state) = 'object'),
    secret_references TEXT[] NOT NULL DEFAULT '{}',
    required_capabilities TEXT[] NOT NULL DEFAULT '{}',
    requester TEXT NOT NULL,
    reason TEXT NOT NULL,
    requested_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    customer_approver TEXT,
    customer_approved_at TIMESTAMPTZ,
    rejected_at TIMESTAMPTZ,
    applied_at TIMESTAMPTZ,
    correlation_id UUID NOT NULL,
    FOREIGN KEY (holding_id, canonical_holding_id)
        REFERENCES erpx_control.customer_holding_bindings (holding_id, canonical_holding_id)
        ON DELETE RESTRICT,
    FOREIGN KEY (target_agent_id)
        REFERENCES erpx_control.data_plane_agents(agent_id) ON DELETE RESTRICT,
    UNIQUE (holding_id, desired_version),
    CHECK (desired_version = expected_observed_version + 1),
    CHECK ((configuration_layer = 'business_unit') = (business_unit IS NOT NULL)),
    CHECK (business_unit IS NULL OR business_unit IN (
        'operations', 'finance', 'sales', 'procurement', 'inventory', 'assets',
        'maintenance', 'quality', 'marketing-crm', 'hr-payroll', 'point-of-sale', 'legal'
    )),
    CHECK ((deployment_mode = 'customer_data_plane') = (target_agent_id IS NOT NULL)),
    CHECK (expires_at > requested_at),
    CHECK (length(btrim(requester)) BETWEEN 1 AND 255),
    CHECK (length(btrim(reason)) BETWEEN 1 AND 512),
    CHECK ((customer_approver IS NULL) = (customer_approved_at IS NULL)),
    CHECK (NOT (customer_approved_at IS NOT NULL AND rejected_at IS NOT NULL))
);

CREATE INDEX IF NOT EXISTS holding_desired_configuration_queue_idx
    ON erpx_control.holding_desired_configurations
        (holding_id, lifecycle, desired_version, requested_at);

CREATE TABLE IF NOT EXISTS erpx_control.data_plane_observations (
    agent_id UUID PRIMARY KEY
        REFERENCES erpx_control.data_plane_agents(agent_id) ON DELETE RESTRICT,
    holding_id TEXT NOT NULL UNIQUE,
    observed_version BIGINT NOT NULL CHECK (observed_version >= 0),
    desired_version BIGINT NOT NULL CHECK (desired_version >= 0),
    write_schema_version TEXT NOT NULL,
    read_schema_version TEXT NOT NULL,
    write_healthy BOOLEAN NOT NULL,
    read_healthy BOOLEAN NOT NULL,
    storage_healthy BOOLEAN NOT NULL,
    directory_ready BOOLEAN NOT NULL,
    projections_healthy BOOLEAN NOT NULL,
    backup_verified BOOLEAN NOT NULL,
    failure_category TEXT,
    correlation_id UUID NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL,
    FOREIGN KEY (holding_id) REFERENCES erpx_control.customer_holding_bindings(holding_id)
        ON DELETE RESTRICT,
    CHECK (length(btrim(write_schema_version)) BETWEEN 1 AND 128),
    CHECK (length(btrim(read_schema_version)) BETWEEN 1 AND 128),
    CHECK (failure_category IS NULL OR length(btrim(failure_category)) BETWEEN 1 AND 255),
    CHECK (failure_category IS NULL OR lower(failure_category) !~ '(password|secret|token|credential)')
);

CREATE TABLE IF NOT EXISTS erpx_control.holding_root_access_requests (
    request_id UUID PRIMARY KEY,
    requester_ref TEXT NOT NULL,
    device_ref TEXT NOT NULL,
    holding_id TEXT NOT NULL,
    canonical_holding_id UUID NOT NULL,
    agent_id UUID NOT NULL,
    holding_scope JSONB NOT NULL CHECK (jsonb_typeof(holding_scope) = 'object'),
    mode TEXT NOT NULL CHECK (mode IN (
        'health_diagnostic', 'infrastructure_maintenance', 'business_diagnostic',
        'business_mutation', 'emergency_recovery'
    )),
    allowed_actions TEXT[] NOT NULL CHECK (cardinality(allowed_actions) BETWEEN 1 AND 64),
    business_units TEXT[] NOT NULL DEFAULT '{}',
    reason TEXT NOT NULL,
    ticket_reference TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'awaiting_customer_approval'
        CHECK (state IN ('awaiting_customer_approval', 'approved', 'rejected', 'active', 'revoked', 'expired')),
    requested_at TIMESTAMPTZ NOT NULL,
    requested_until TIMESTAMPTZ NOT NULL,
    customer_approver_ref TEXT,
    customer_approved_at TIMESTAMPTZ,
    customer_mfa_at TIMESTAMPTZ,
    rejected_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    revocation_reason TEXT,
    correlation_id UUID NOT NULL,
    FOREIGN KEY (holding_id, canonical_holding_id)
        REFERENCES erpx_control.customer_holding_bindings (holding_id, canonical_holding_id)
        ON DELETE RESTRICT,
    FOREIGN KEY (agent_id) REFERENCES erpx_control.data_plane_agents(agent_id)
        ON DELETE RESTRICT,
    CHECK (requested_until > requested_at),
    CHECK (length(btrim(requester_ref)) BETWEEN 1 AND 255),
    CHECK (length(btrim(device_ref)) BETWEEN 1 AND 255),
    CHECK (length(btrim(reason)) BETWEEN 1 AND 512),
    CHECK (length(btrim(ticket_reference)) BETWEEN 1 AND 255),
    CHECK (customer_approver_ref IS NULL OR customer_approver_ref <> requester_ref),
    CHECK ((customer_approver_ref IS NULL) = (customer_approved_at IS NULL)),
    CHECK (customer_approved_at IS NULL OR customer_mfa_at IS NOT NULL),
    CHECK (NOT (customer_approved_at IS NOT NULL AND rejected_at IS NOT NULL)),
    CHECK ((revoked_at IS NULL) = (revocation_reason IS NULL))
);

CREATE INDEX IF NOT EXISTS holding_root_access_approval_queue_idx
    ON erpx_control.holding_root_access_requests
        (holding_id, state, requested_until, requested_at);

CREATE TABLE IF NOT EXISTS erpx_control.holding_root_access_capabilities (
    capability_id UUID PRIMARY KEY,
    request_id UUID NOT NULL UNIQUE
        REFERENCES erpx_control.holding_root_access_requests(request_id) ON DELETE RESTRICT,
    token_hash BYTEA NOT NULL UNIQUE CHECK (octet_length(token_hash) = 32),
    root_principal_ref TEXT NOT NULL,
    device_ref TEXT NOT NULL,
    holding_id TEXT NOT NULL,
    agent_id UUID NOT NULL,
    holding_scope JSONB NOT NULL CHECK (jsonb_typeof(holding_scope) = 'object'),
    allowed_actions TEXT[] NOT NULL CHECK (cardinality(allowed_actions) BETWEEN 1 AND 64),
    business_units TEXT[] NOT NULL DEFAULT '{}',
    issued_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    CHECK (expires_at > issued_at),
    CHECK (length(btrim(root_principal_ref)) BETWEEN 1 AND 255),
    CHECK (length(btrim(device_ref)) BETWEEN 1 AND 255),
    FOREIGN KEY (holding_id) REFERENCES erpx_control.customer_holding_bindings(holding_id)
        ON DELETE RESTRICT,
    FOREIGN KEY (agent_id) REFERENCES erpx_control.data_plane_agents(agent_id)
        ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS holding_root_access_capability_active_idx
    ON erpx_control.holding_root_access_capabilities
        (holding_id, root_principal_ref, agent_id, expires_at)
    WHERE revoked_at IS NULL;

CREATE TABLE IF NOT EXISTS erpx_control.business_unit_configurations (
    holding_id TEXT NOT NULL,
    canonical_holding_id UUID NOT NULL,
    business_unit TEXT NOT NULL CHECK (business_unit IN (
        'operations', 'finance', 'sales', 'procurement', 'inventory', 'assets',
        'maintenance', 'quality', 'marketing-crm', 'hr-payroll', 'point-of-sale', 'legal'
    )),
    configuration_version BIGINT NOT NULL CHECK (configuration_version > 0),
    active BOOLEAN NOT NULL,
    organization_scope JSONB NOT NULL CHECK (jsonb_typeof(organization_scope) = 'object'),
    directory_role_mappings JSONB NOT NULL DEFAULT '[]'::jsonb
        CHECK (jsonb_typeof(directory_role_mappings) = 'array'),
    integration_references TEXT[] NOT NULL DEFAULT '{}',
    retention_policy JSONB NOT NULL DEFAULT '{}'::jsonb
        CHECK (jsonb_typeof(retention_policy) = 'object'),
    projection_policy JSONB NOT NULL DEFAULT '{}'::jsonb
        CHECK (jsonb_typeof(projection_policy) = 'object'),
    workflow_policy JSONB NOT NULL DEFAULT '{}'::jsonb
        CHECK (jsonb_typeof(workflow_policy) = 'object'),
    updated_by TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_id, business_unit),
    FOREIGN KEY (holding_id, canonical_holding_id)
        REFERENCES erpx_control.customer_holding_bindings (holding_id, canonical_holding_id)
        ON DELETE RESTRICT,
    CHECK (length(btrim(updated_by)) BETWEEN 1 AND 255)
);


-- Governed corporate identity is public presentation metadata in the ERPX
-- control plane. It is intentionally independent of customer-hosted data
-- planes and stores only typed token JSON and immutable asset references.

CREATE TABLE IF NOT EXISTS erpx_control.corporate_identity_profiles (
    profile_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL UNIQUE,
    scope_kind TEXT NOT NULL
        CHECK (scope_kind IN ('platform', 'holding', 'business_unit')),
    holding_id TEXT,
    canonical_holding_id UUID,
    business_unit TEXT,
    current_draft_version BIGINT NOT NULL DEFAULT 0
        CHECK (current_draft_version >= 0),
    current_published_version BIGINT
        CHECK (current_published_version IS NULL OR current_published_version > 0),
    suspended_at TIMESTAMPTZ,
    suspension_reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    FOREIGN KEY (holding_id, canonical_holding_id)
        REFERENCES erpx_control.customer_holding_bindings (holding_id, canonical_holding_id)
        ON DELETE RESTRICT,
    CHECK (
        (scope_kind = 'platform'
            AND holding_id IS NULL
            AND canonical_holding_id IS NULL
            AND business_unit IS NULL)
        OR
        (scope_kind = 'holding'
            AND holding_id IS NOT NULL
            AND canonical_holding_id IS NOT NULL
            AND business_unit IS NULL)
        OR
        (scope_kind = 'business_unit'
            AND holding_id IS NOT NULL
            AND canonical_holding_id IS NOT NULL
            AND business_unit IS NOT NULL)
    ),
    CHECK (business_unit IS NULL OR business_unit IN (
        'operations', 'finance', 'sales', 'procurement', 'inventory', 'assets',
        'maintenance', 'quality', 'marketing-crm', 'hr-payroll', 'point-of-sale', 'legal'
    )),
    CHECK ((suspended_at IS NULL) = (suspension_reason IS NULL)),
    CHECK (suspension_reason IS NULL
        OR length(btrim(suspension_reason)) BETWEEN 1 AND 512)
);

CREATE UNIQUE INDEX IF NOT EXISTS corporate_identity_platform_profile_idx
    ON erpx_control.corporate_identity_profiles (scope_kind)
    WHERE scope_kind = 'platform';

CREATE UNIQUE INDEX IF NOT EXISTS corporate_identity_holding_profile_idx
    ON erpx_control.corporate_identity_profiles (holding_id, canonical_holding_id)
    WHERE scope_kind = 'holding';

CREATE UNIQUE INDEX IF NOT EXISTS corporate_identity_business_unit_profile_idx
    ON erpx_control.corporate_identity_profiles
        (holding_id, canonical_holding_id, business_unit)
    WHERE scope_kind = 'business_unit';

CREATE TABLE IF NOT EXISTS erpx_control.corporate_identity_versions (
    profile_id UUID NOT NULL
        REFERENCES erpx_control.corporate_identity_profiles(profile_id) ON DELETE RESTRICT,
    version BIGINT NOT NULL CHECK (version > 0),
    lifecycle TEXT NOT NULL
        CHECK (lifecycle IN ('draft', 'published', 'archived', 'suspended')),
    parent_published_version BIGINT,
    display_name TEXT NOT NULL,
    short_name TEXT NOT NULL,
    tokens JSONB NOT NULL CHECK (jsonb_typeof(tokens) = 'object'),
    assets JSONB NOT NULL CHECK (jsonb_typeof(assets) = 'object'),
    overridden_categories TEXT[] NOT NULL DEFAULT '{}',
    locked_categories TEXT[] NOT NULL DEFAULT '{}',
    updated_by TEXT NOT NULL,
    reason TEXT NOT NULL,
    correlation_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    published_at TIMESTAMPTZ,
    archived_at TIMESTAMPTZ,
    PRIMARY KEY (profile_id, version),
    CHECK (length(btrim(display_name)) BETWEEN 1 AND 96),
    CHECK (length(btrim(short_name)) BETWEEN 1 AND 24),
    CHECK (length(btrim(updated_by)) BETWEEN 1 AND 255),
    CHECK (length(btrim(reason)) BETWEEN 1 AND 512),
    CHECK (parent_published_version IS NULL OR parent_published_version > 0),
    CHECK (overridden_categories <@ ARRAY[
        'identity', 'color', 'typography', 'shape', 'elevation',
        'density', 'navigation', 'motion'
    ]::TEXT[]),
    CHECK (locked_categories <@ ARRAY[
        'identity', 'color', 'typography', 'shape', 'elevation',
        'density', 'navigation', 'motion'
    ]::TEXT[]),
    CHECK (lifecycle <> 'published' OR published_at IS NOT NULL),
    CHECK (lifecycle <> 'archived' OR archived_at IS NOT NULL)
);

CREATE UNIQUE INDEX IF NOT EXISTS corporate_identity_one_draft_idx
    ON erpx_control.corporate_identity_versions(profile_id)
    WHERE lifecycle = 'draft';

CREATE UNIQUE INDEX IF NOT EXISTS corporate_identity_one_published_idx
    ON erpx_control.corporate_identity_versions(profile_id)
    WHERE lifecycle = 'published';

CREATE INDEX IF NOT EXISTS corporate_identity_history_idx
    ON erpx_control.corporate_identity_versions(profile_id, version DESC);

CREATE TABLE IF NOT EXISTS erpx_control.corporate_identity_assets (
    asset_id UUID PRIMARY KEY,
    profile_id UUID NOT NULL
        REFERENCES erpx_control.corporate_identity_profiles(profile_id) ON DELETE RESTRICT,
    asset_kind TEXT NOT NULL
        CHECK (asset_kind IN ('logo_mark', 'wordmark', 'favicon', 'login_artwork')),
    object_key TEXT NOT NULL UNIQUE,
    sha256 CHAR(64) NOT NULL CHECK (sha256 ~ '^[0-9A-Fa-f]{64}$'),
    media_type TEXT NOT NULL
        CHECK (media_type IN ('image/png', 'image/webp', 'image/jpeg', 'image/svg+xml')),
    byte_length BIGINT NOT NULL CHECK (byte_length BETWEEN 1 AND 5242880),
    width INTEGER CHECK (width BETWEEN 1 AND 4096),
    height INTEGER CHECK (height BETWEEN 1 AND 4096),
    validation_state TEXT NOT NULL
        CHECK (validation_state IN ('pending', 'valid', 'rejected')),
    validation_reason TEXT,
    uploaded_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    validated_at TIMESTAMPTZ,
    CHECK (object_key LIKE 'branding/%'),
    CHECK (object_key NOT LIKE '%..%'),
    CHECK (position('://' IN object_key) = 0),
    CHECK (length(btrim(uploaded_by)) BETWEEN 1 AND 255),
    CHECK ((validation_state = 'pending') = (validated_at IS NULL)),
    CHECK (validation_reason IS NULL
        OR length(btrim(validation_reason)) BETWEEN 1 AND 512)
);

CREATE INDEX IF NOT EXISTS corporate_identity_asset_profile_idx
    ON erpx_control.corporate_identity_assets(profile_id, validation_state, asset_kind);

CREATE TABLE IF NOT EXISTS erpx_control.corporate_identity_domains (
    hostname TEXT PRIMARY KEY,
    profile_id UUID NOT NULL
        REFERENCES erpx_control.corporate_identity_profiles(profile_id) ON DELETE RESTRICT,
    verification_digest BYTEA NOT NULL CHECK (octet_length(verification_digest) = 32),
    verified_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (hostname = lower(hostname)),
    CHECK (hostname !~ '[/\\?#:]'),
    CHECK (length(hostname) BETWEEN 1 AND 253),
    CHECK (NOT (verified_at IS NOT NULL AND revoked_at IS NOT NULL))
);

CREATE INDEX IF NOT EXISTS corporate_identity_verified_domain_idx
    ON erpx_control.corporate_identity_domains(profile_id, hostname)
    WHERE verified_at IS NOT NULL AND revoked_at IS NULL;

-- The neutral platform identity is a normal published version so every
-- holding can inherit immediately without a destructive data migration.
INSERT INTO erpx_control.corporate_identity_profiles
    (profile_id, scope_key, scope_kind, current_draft_version,
     current_published_version, created_at, updated_at)
VALUES
    ('00000000-0000-0000-0000-000000000071', 'platform', 'platform', 0, 1, now(), now())
ON CONFLICT (scope_key) DO NOTHING;

INSERT INTO erpx_control.corporate_identity_versions
    (profile_id, version, lifecycle, display_name, short_name, tokens, assets,
     overridden_categories, locked_categories, updated_by, reason,
     correlation_id, created_at, published_at)
VALUES (
    '00000000-0000-0000-0000-000000000071',
    1,
    'published',
    'ERPX Platform',
    'ERPX',
    '{
      "light":{"primary":"#075985","on_primary":"#ffffff","accent":"#0f766e","on_accent":"#ffffff","canvas":"#f5f7f9","surface":"#ffffff","surface_raised":"#edf2f5","border":"#7b8b98","text":"#111827","text_muted":"#4b5b68","link":"#075985","focus":"#075985"},
      "dark":{"primary":"#67e8f9","on_primary":"#082f49","accent":"#5eead4","on_accent":"#042f2e","canvas":"#0b1117","surface":"#111a22","surface_raised":"#1b2833","border":"#687b89","text":"#f1f5f9","text_muted":"#a8bac7","link":"#67e8f9","focus":"#a5f3fc"},
      "font_pack":"erpx_sans","heading_scale_percent":100,"body_scale_percent":100,
      "line_height_percent":150,"density":"comfortable","radius":"soft",
      "elevation":"subtle","navigation":"tinted",
      "navigation_expanded_by_default":true,"motion":"subtle"
    }'::jsonb,
    '{}'::jsonb,
    ARRAY['identity','color','typography','shape','elevation','density','navigation','motion'],
    ARRAY[]::TEXT[],
    'migration:0071',
    'neutral ERPX corporate identity bootstrap',
    '00000000-0000-0000-0000-000000000071',
    now(),
    now()
)
ON CONFLICT (profile_id, version) DO NOTHING;


-- Branch-scoped physical database routing and resumable first-install state.
-- All credential material remains in server-side profile resolvers; these
-- relations contain only bounded profile keys, opaque physical identifiers,
-- lifecycle state, and redacted observations.

ALTER TABLE erpx_control.holding_registry
    ADD COLUMN IF NOT EXISTS isolation_mode TEXT NOT NULL DEFAULT 'holding_pair';

ALTER TABLE erpx_control.holding_registry
    DROP CONSTRAINT IF EXISTS holding_registry_isolation_mode_check;
ALTER TABLE erpx_control.holding_registry
    ADD CONSTRAINT holding_registry_isolation_mode_check
    CHECK (isolation_mode IN ('holding_pair', 'holding_and_branch_pairs'));

CREATE TABLE IF NOT EXISTS erpx_control.database_profiles (
    profile_key TEXT PRIMARY KEY,
    database_prefix TEXT NOT NULL,
    deployment_mode TEXT NOT NULL
        CHECK (deployment_mode IN ('erpx_managed', 'customer_data_plane')),
    credential_profile_key TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('active', 'suspended')),
    profile_version BIGINT NOT NULL CHECK (profile_version > 0),
    created_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (profile_key ~ '^[a-z][a-z0-9_-]{0,63}$'),
    CHECK (database_prefix ~ '^[a-z][a-z0-9_]{1,15}$'),
    CHECK (length(btrim(credential_profile_key)) BETWEEN 1 AND 255),
    CHECK (length(btrim(created_by)) BETWEEN 1 AND 255)
);

CREATE TABLE IF NOT EXISTS erpx_control.holding_database_profile_bindings (
    holding_id TEXT NOT NULL
        REFERENCES erpx_control.holding_registry(holding_id) ON DELETE RESTRICT,
    profile_key TEXT NOT NULL
        REFERENCES erpx_control.database_profiles(profile_key) ON DELETE RESTRICT,
    active BOOLEAN NOT NULL DEFAULT TRUE,
    bound_by TEXT NOT NULL,
    bound_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_id, profile_key),
    CHECK (length(btrim(bound_by)) BETWEEN 1 AND 255)
);

CREATE TABLE IF NOT EXISTS erpx_control.branch_database_routes (
    holding_id TEXT NOT NULL,
    data_profile_key TEXT NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID NOT NULL,
    profile_key TEXT NOT NULL,
    write_database_key TEXT NOT NULL,
    read_database_key TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN (
        'queued', 'provisioning', 'verifying', 'active', 'failed', 'suspended'
    )),
    route_version BIGINT NOT NULL CHECK (route_version > 0),
    failure_category TEXT,
    requested_by TEXT NOT NULL,
    requested_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    activated_at TIMESTAMPTZ,
    suspended_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_id, branch_id),
    FOREIGN KEY (holding_id, data_profile_key)
        REFERENCES erpx_control.holding_registry(holding_id, data_profile_key)
        ON DELETE RESTRICT,
    FOREIGN KEY (holding_id, profile_key)
        REFERENCES erpx_control.holding_database_profile_bindings(holding_id, profile_key)
        ON DELETE RESTRICT,
    UNIQUE (write_database_key),
    UNIQUE (read_database_key),
    CHECK (write_database_key <> read_database_key),
    CHECK (length(btrim(write_database_key)) BETWEEN 1 AND 63),
    CHECK (length(btrim(read_database_key)) BETWEEN 1 AND 63),
    CHECK (write_database_key ~ '^[A-Za-z0-9_-]+$'),
    CHECK (read_database_key ~ '^[A-Za-z0-9_-]+$'),
    CHECK (length(btrim(requested_by)) BETWEEN 1 AND 255),
    CHECK (failure_category IS NULL OR (
        length(btrim(failure_category)) BETWEEN 1 AND 128
        AND lower(failure_category) !~ '(password|secret|token|credential|postgres://|postgresql://)'
    )),
    CHECK ((state = 'active') = (activated_at IS NOT NULL)),
    CHECK ((state = 'suspended') = (suspended_at IS NOT NULL))
);

CREATE INDEX IF NOT EXISTS branch_database_routes_parent_idx
    ON erpx_control.branch_database_routes
        (holding_id, company_id, state, branch_id);

CREATE TABLE IF NOT EXISTS erpx_control.branch_provisioning_jobs (
    job_id UUID PRIMARY KEY,
    holding_id TEXT NOT NULL,
    branch_id UUID NOT NULL,
    idempotency_key TEXT NOT NULL,
    branch_code TEXT NOT NULL,
    branch_name TEXT NOT NULL,
    registration_number TEXT NOT NULL DEFAULT '',
    enabled_business_units TEXT[] NOT NULL,
    state TEXT NOT NULL CHECK (state IN (
        'queued', 'provisioning', 'verifying', 'active', 'failed', 'suspended'
    )),
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count BETWEEN 0 AND 10),
    claimed_by UUID,
    claimed_at TIMESTAMPTZ,
    lease_expires_at TIMESTAMPTZ,
    next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    failure_category TEXT,
    correlation_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ,
    FOREIGN KEY (holding_id, branch_id)
        REFERENCES erpx_control.branch_database_routes(holding_id, branch_id)
        ON DELETE RESTRICT,
    UNIQUE (holding_id, idempotency_key),
    CHECK (length(btrim(idempotency_key)) BETWEEN 1 AND 255),
    CHECK (length(btrim(branch_code)) BETWEEN 1 AND 128),
    CHECK (length(btrim(branch_name)) BETWEEN 1 AND 255),
    CHECK (length(registration_number) <= 255),
    CHECK (cardinality(enabled_business_units) BETWEEN 1 AND 12),
    CHECK (enabled_business_units <@ ARRAY[
        'operations', 'finance', 'sales', 'procurement', 'inventory', 'assets',
        'maintenance', 'quality', 'marketing-crm', 'hr-payroll',
        'point-of-sale', 'legal'
    ]::TEXT[]),
    CHECK ((claimed_by IS NULL) = (claimed_at IS NULL)),
    CHECK ((claimed_by IS NULL) = (lease_expires_at IS NULL)),
    CHECK (lease_expires_at IS NULL OR lease_expires_at > claimed_at),
    CHECK (failure_category IS NULL OR (
        length(btrim(failure_category)) BETWEEN 1 AND 128
        AND lower(failure_category) !~ '(password|secret|token|credential|postgres://|postgresql://)'
    )),
    CHECK ((state = 'active') = (completed_at IS NOT NULL))
);

CREATE INDEX IF NOT EXISTS branch_provisioning_jobs_claim_idx
    ON erpx_control.branch_provisioning_jobs (next_attempt_at, created_at, job_id)
    WHERE state = 'queued' AND completed_at IS NULL;

CREATE TABLE IF NOT EXISTS erpx_control.branch_database_health (
    holding_id TEXT NOT NULL,
    branch_id UUID NOT NULL,
    side TEXT NOT NULL CHECK (side IN ('write', 'read')),
    database_key TEXT NOT NULL,
    schema_version TEXT NOT NULL,
    reachable BOOLEAN NOT NULL,
    checked_at TIMESTAMPTZ NOT NULL,
    failure_category TEXT,
    PRIMARY KEY (holding_id, branch_id, side),
    FOREIGN KEY (holding_id, branch_id)
        REFERENCES erpx_control.branch_database_routes(holding_id, branch_id)
        ON DELETE RESTRICT,
    CHECK (length(btrim(schema_version)) BETWEEN 1 AND 128),
    CHECK (failure_category IS NULL OR (
        length(btrim(failure_category)) BETWEEN 1 AND 128
        AND lower(failure_category) !~ '(password|secret|token|credential|postgres://|postgresql://)'
    ))
);

CREATE TABLE IF NOT EXISTS erpx_control.branch_business_unit_configurations (
    holding_id TEXT NOT NULL,
    branch_id UUID NOT NULL,
    business_unit TEXT NOT NULL CHECK (business_unit IN (
        'operations', 'finance', 'sales', 'procurement', 'inventory', 'assets',
        'maintenance', 'quality', 'marketing-crm', 'hr-payroll',
        'point-of-sale', 'legal'
    )),
    active BOOLEAN NOT NULL,
    configuration_version BIGINT NOT NULL CHECK (configuration_version > 0),
    updated_by TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_id, branch_id, business_unit),
    FOREIGN KEY (holding_id, branch_id)
        REFERENCES erpx_control.branch_database_routes(holding_id, branch_id)
        ON DELETE RESTRICT,
    CHECK (length(btrim(updated_by)) BETWEEN 1 AND 255)
);

-- Existing holding-pair holdings opt in one branch at a time. The job retains
-- only database keys, high-watermarks, counts, digests and rollback fences;
-- aggregate payloads continue to live in holding databases and their outboxes.
CREATE TABLE IF NOT EXISTS erpx_control.branch_route_migration_jobs (
    migration_id UUID PRIMARY KEY,
    holding_id TEXT NOT NULL,
    branch_id UUID NOT NULL,
    state TEXT NOT NULL CHECK (state IN (
        'queued', 'preflighting', 'provisioning', 'copying', 'write_fenced',
        'verifying', 'active', 'rollback_queued', 'rolling_back',
        'rolled_back', 'failed'
    )),
    source_write_database_key TEXT NOT NULL,
    source_read_database_key TEXT NOT NULL,
    candidate_write_database_key TEXT NOT NULL,
    candidate_read_database_key TEXT NOT NULL,
    source_high_watermark BIGINT NOT NULL DEFAULT 0 CHECK (source_high_watermark >= 0),
    final_high_watermark BIGINT CHECK (final_high_watermark >= source_high_watermark),
    aggregate_count BIGINT CHECK (aggregate_count IS NULL OR aggregate_count >= 0),
    receipt_count BIGINT CHECK (receipt_count IS NULL OR receipt_count >= 0),
    outbox_count BIGINT CHECK (outbox_count IS NULL OR outbox_count >= 0),
    source_digest CHAR(64),
    candidate_digest CHAR(64),
    requested_by TEXT NOT NULL,
    approved_by TEXT NOT NULL,
    correlation_id UUID NOT NULL,
    failure_category TEXT,
    rollback_until TIMESTAMPTZ,
    activated_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    FOREIGN KEY (holding_id, branch_id)
        REFERENCES erpx_control.branch_database_routes(holding_id, branch_id)
        ON DELETE RESTRICT,
    UNIQUE (holding_id, branch_id, migration_id),
    CHECK (requested_by <> approved_by),
    CHECK (length(btrim(requested_by)) BETWEEN 1 AND 255),
    CHECK (length(btrim(approved_by)) BETWEEN 1 AND 255),
    CHECK (source_write_database_key <> candidate_write_database_key),
    CHECK (source_read_database_key <> candidate_read_database_key),
    CHECK (source_digest IS NULL OR source_digest ~ '^[0-9A-Fa-f]{64}$'),
    CHECK (candidate_digest IS NULL OR candidate_digest ~ '^[0-9A-Fa-f]{64}$'),
    CHECK ((state = 'active') = (activated_at IS NOT NULL AND rollback_until IS NOT NULL)),
    CHECK (rollback_until IS NULL OR rollback_until > activated_at),
    CHECK (failure_category IS NULL OR (
        length(btrim(failure_category)) BETWEEN 1 AND 128
        AND lower(failure_category) !~ '(password|secret|token|credential|postgres://|postgresql://)'
    ))
);

CREATE UNIQUE INDEX IF NOT EXISTS branch_route_migration_jobs_open_idx
    ON erpx_control.branch_route_migration_jobs (holding_id, branch_id)
    WHERE state NOT IN ('active', 'rolled_back', 'failed');

CREATE INDEX IF NOT EXISTS branch_route_migration_jobs_claim_idx
    ON erpx_control.branch_route_migration_jobs (state, updated_at, migration_id)
    WHERE state IN ('queued', 'rollback_queued');

CREATE TABLE IF NOT EXISTS erpx_control.platform_bootstrap_receipts (
    installation_prefix TEXT PRIMARY KEY,
    holding_id TEXT NOT NULL UNIQUE
        REFERENCES erpx_control.holding_registry(holding_id) ON DELETE RESTRICT,
    data_profile_key TEXT NOT NULL,
    root_principal_ref TEXT NOT NULL,
    owner_principal_ref TEXT NOT NULL,
    profile_key TEXT NOT NULL
        REFERENCES erpx_control.database_profiles(profile_key) ON DELETE RESTRICT,
    state TEXT NOT NULL CHECK (state IN ('provisioning', 'complete', 'failed')),
    completed_steps TEXT[] NOT NULL DEFAULT '{}',
    failure_category TEXT,
    bootstrap_version BIGINT NOT NULL CHECK (bootstrap_version > 0),
    started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (installation_prefix ~ '^[a-z][a-z0-9_]{1,15}$'),
    CHECK (root_principal_ref <> owner_principal_ref),
    CHECK (length(btrim(root_principal_ref)) BETWEEN 1 AND 255),
    CHECK (length(btrim(owner_principal_ref)) BETWEEN 1 AND 255),
    CHECK ((state = 'complete') = (completed_at IS NOT NULL)),
    CHECK (failure_category IS NULL OR (
        length(btrim(failure_category)) BETWEEN 1 AND 128
        AND lower(failure_category) !~ '(password|secret|token|credential|postgres://|postgresql://)'
    ))
);


-- Durable, credential-free health observations for the Platform Integrations
-- operator surface.  The table stores categorical state only; endpoints,
-- database keys, provider payloads, and credentials are deliberately excluded.

CREATE TABLE IF NOT EXISTS erpx_control.platform_integration_observations (
    integration_key TEXT PRIMARY KEY,
    state TEXT NOT NULL CHECK (state IN (
        'ready', 'not_ready', 'configured', 'not_configured',
        'healthy', 'degraded', 'unavailable', 'not_probed'
    )),
    capability TEXT NOT NULL,
    checked_at TIMESTAMPTZ NOT NULL,
    observed_by TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (integration_key ~ '^[a-z][a-z0-9_]{1,63}$'),
    CHECK (length(btrim(capability)) BETWEEN 1 AND 256),
    CHECK (length(btrim(observed_by)) BETWEEN 1 AND 255)
);

CREATE INDEX IF NOT EXISTS platform_integration_observations_checked_idx
    ON erpx_control.platform_integration_observations (checked_at DESC, integration_key);

INSERT INTO erpx_control.platform_integration_observations
    (integration_key, state, capability, checked_at, observed_by)
VALUES
    ('control_plane', 'not_probed', 'SQLx control-plane write/read pools', now(), 'migration'),
    ('identity', 'not_probed', 'SQLx identity, session, and recovery boundary', now(), 'migration'),
    ('holding_routing', 'not_probed', 'Physical holding write/read database routing', now(), 'migration'),
    ('object_storage', 'not_probed', 'MinIO-compatible holding object storage', now(), 'migration'),
    ('billing', 'not_probed', 'Stripe catalog, webhook, and entitlement boundary', now(), 'migration')
ON CONFLICT (integration_key) DO NOTHING;


-- Explicit authorization targets for local role assignments. Every existing
-- assignment is backfilled as unrestricted to preserve production behavior;
-- new authorization code denies assignments with no policy row.

CREATE TABLE IF NOT EXISTS erpx_control.role_assignment_target_policies (
    assignment_id UUID PRIMARY KEY
        REFERENCES erpx_control.role_assignments(id) ON DELETE RESTRICT,
    policy_kind TEXT NOT NULL
        CHECK (policy_kind IN ('unrestricted', 'holding_allowlist')),
    environment_gate TEXT,
    configured_by TEXT NOT NULL,
    configured_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (length(btrim(configured_by)) BETWEEN 1 AND 255),
    CHECK (
        (policy_kind = 'unrestricted' AND environment_gate IS NULL)
        OR
        (policy_kind = 'holding_allowlist'
         AND environment_gate = 'local-ucs-lab')
    )
);

CREATE TABLE IF NOT EXISTS erpx_control.role_assignment_holding_targets (
    assignment_id UUID NOT NULL
        REFERENCES erpx_control.role_assignment_target_policies(assignment_id)
        ON DELETE CASCADE,
    holding_id TEXT NOT NULL,
    configured_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (assignment_id, holding_id),
    CHECK (length(btrim(holding_id)) BETWEEN 1 AND 128),
    CHECK (holding_id !~ '[[:cntrl:]]')
);

CREATE INDEX IF NOT EXISTS role_assignment_holding_targets_holding_idx
    ON erpx_control.role_assignment_holding_targets (holding_id, assignment_id);

INSERT INTO erpx_control.role_assignment_target_policies
    (assignment_id, policy_kind, environment_gate, configured_by)
SELECT id, 'unrestricted', NULL, 'migration:0074'
FROM erpx_control.role_assignments
ON CONFLICT (assignment_id) DO NOTHING;

-- The existing audit table is the authoritative, payload-free source for the
-- holding control history cursor.
CREATE INDEX IF NOT EXISTS holding_platform_access_audit_history_cursor_idx
    ON erpx_control.holding_platform_access_audit
        (holding_id, occurred_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS control_outbox_holding_history_cursor_idx
    ON erpx_control.outbox_events
        ((active_scope->>'holding_id'), occurred_at DESC, id DESC)
    WHERE active_scope ? 'holding_id';


-- Control-plane CQRS storage is capability-separated from holding write
-- databases. Only platform-scoped aggregates belong here; holding business
-- aggregates continue to use the holding-write migration set.

CREATE SCHEMA IF NOT EXISTS erpx_cqrs_write;

CREATE TABLE erpx_cqrs_write.command_receipts (
    context_name TEXT NOT NULL,
    partition_key TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    command_id UUID NOT NULL,
    request_hash BYTEA NOT NULL CHECK (octet_length(request_hash) = 32),
    correlation_id UUID NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('reserved', 'completed')),
    receipt JSONB,
    created_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,
    PRIMARY KEY (context_name, partition_key, idempotency_key),
    UNIQUE (context_name, partition_key, command_id),
    CHECK (
        (status = 'reserved' AND receipt IS NULL AND completed_at IS NULL)
        OR (status = 'completed' AND receipt IS NOT NULL AND completed_at IS NOT NULL)
    )
);

CREATE TABLE erpx_cqrs_write.audit_log (
    audit_id UUID PRIMARY KEY,
    context_name TEXT NOT NULL,
    partition_key TEXT NOT NULL,
    command_id UUID NOT NULL,
    correlation_id UUID NOT NULL,
    active_scope JSONB NOT NULL,
    actor JSONB NOT NULL,
    audit_context JSONB NOT NULL,
    action TEXT NOT NULL,
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    details JSONB NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    UNIQUE (context_name, partition_key, command_id, audit_id)
);

CREATE TABLE erpx_cqrs_write.outbox_events (
    global_sequence BIGSERIAL PRIMARY KEY,
    event_id UUID NOT NULL UNIQUE,
    context_name TEXT NOT NULL,
    partition_key TEXT NOT NULL,
    command_id UUID NOT NULL,
    event_type TEXT NOT NULL,
    event_version INTEGER NOT NULL CHECK (event_version > 0),
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    active_scope JSONB NOT NULL,
    actor JSONB NOT NULL,
    correlation_id UUID NOT NULL,
    causation_id UUID,
    payload JSONB NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    available_at TIMESTAMPTZ NOT NULL,
    published_at TIMESTAMPTZ,
    publish_attempts INTEGER NOT NULL DEFAULT 0 CHECK (publish_attempts >= 0),
    publish_error TEXT
);

CREATE INDEX cqrs_control_outbox_projection_source
    ON erpx_cqrs_write.outbox_events (global_sequence)
    WHERE published_at IS NULL;

CREATE INDEX cqrs_control_outbox_publish_source
    ON erpx_cqrs_write.outbox_events (available_at, global_sequence)
    WHERE published_at IS NULL;

CREATE INDEX cqrs_control_outbox_command
    ON erpx_cqrs_write.outbox_events (context_name, partition_key, command_id);

CREATE SCHEMA IF NOT EXISTS erpx_commercial_write;

CREATE TABLE erpx_commercial_write.catalog_plan_guards (
    catalog_plan_id UUID PRIMARY KEY,
    plan_key TEXT NOT NULL UNIQUE,
    plan_status TEXT NOT NULL CHECK (plan_status IN ('draft', 'published')),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL
);


-- Wave 5 control-plane guard/root tables. Migration 0082 completes their
-- typed relational authority; receipts, audit, and outbox remain durable CQRS
-- facts committed in the same transaction.

CREATE SCHEMA IF NOT EXISTS erpx_platform_administration_write;

CREATE TABLE erpx_platform_administration_write.holding_guards (
    holding_id UUID PRIMARY KEY,
    holding_code TEXT NOT NULL UNIQUE,
    holding_name TEXT NOT NULL,
    environment_id UUID NOT NULL,
    customer_id UUID NOT NULL UNIQUE,
    canonical_holding_id UUID NOT NULL UNIQUE,
    data_profile_key TEXT NOT NULL UNIQUE,
    lifecycle_state TEXT NOT NULL CHECK (lifecycle_state IN (
        'pending_subscription', 'provisioning', 'provisioning_failed',
        'active', 'suspended', 'recovery', 'retired'
    )),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX platform_admin_holding_state
    ON erpx_platform_administration_write.holding_guards
        (lifecycle_state, updated_at DESC, holding_id DESC);

CREATE TABLE erpx_platform_administration_write.connector_guards (
    connector_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    connector_kind TEXT NOT NULL,
    connector_key TEXT NOT NULL,
    configuration_digest BYTEA NOT NULL CHECK (octet_length(configuration_digest) = 32),
    capabilities JSONB NOT NULL,
    connector_state TEXT NOT NULL CHECK (connector_state IN (
        'pending', 'active', 'suspended', 'retired'
    )),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE (scope_key, connector_key)
);

CREATE INDEX platform_admin_connector_scope_state
    ON erpx_platform_administration_write.connector_guards
        (scope_key, connector_state, updated_at DESC, connector_id DESC);

CREATE TABLE erpx_platform_administration_write.admin_context_guards (
    context_id UUID PRIMARY KEY,
    holding_id UUID NOT NULL,
    scope_key TEXT NOT NULL,
    principal_ref TEXT NOT NULL,
    device_id UUID NOT NULL,
    session_id UUID NOT NULL,
    reason TEXT NOT NULL,
    issued_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    CHECK (expires_at > issued_at),
    CHECK (expires_at <= issued_at + INTERVAL '30 minutes')
);

CREATE INDEX platform_admin_context_live
    ON erpx_platform_administration_write.admin_context_guards
        (holding_id, scope_key, expires_at, context_id)
    WHERE revoked_at IS NULL;

CREATE SCHEMA IF NOT EXISTS erpx_brand_write;

CREATE TABLE erpx_brand_write.profile_guards (
    profile_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL UNIQUE,
    draft_revision BIGINT CHECK (draft_revision > 0),
    published_revision BIGINT CHECK (published_revision > 0),
    parent_published_revision BIGINT CHECK (parent_published_revision > 0),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE erpx_brand_write.profile_revisions (
    profile_id UUID NOT NULL,
    revision BIGINT NOT NULL CHECK (revision > 0),
    lifecycle TEXT NOT NULL CHECK (lifecycle IN ('draft', 'published')),
    definition JSONB NOT NULL,
    parent_published_revision BIGINT CHECK (parent_published_revision > 0),
    updated_by TEXT NOT NULL,
    reason TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    published_at TIMESTAMPTZ,
    archived_at TIMESTAMPTZ,
    PRIMARY KEY (profile_id, revision),
    FOREIGN KEY (profile_id)
        REFERENCES erpx_brand_write.profile_guards (profile_id)
        ON DELETE CASCADE
);

CREATE INDEX brand_revision_history
    ON erpx_brand_write.profile_revisions
        (profile_id, revision DESC, created_at DESC);

CREATE SCHEMA IF NOT EXISTS erpx_customer_authority_write;

CREATE TABLE erpx_customer_authority_write.account_guards (
    customer_id UUID PRIMARY KEY,
    customer_code TEXT NOT NULL UNIQUE,
    primary_scope_key TEXT NOT NULL,
    primary_holding_id UUID NOT NULL,
    primary_company_id UUID NOT NULL,
    customer_status TEXT NOT NULL CHECK (customer_status IN (
        'prospect', 'onboarding', 'active', 'suspended', 'churned'
    )),
    merged_into_customer_id UUID,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (merged_into_customer_id IS NULL OR merged_into_customer_id <> customer_id)
);

CREATE INDEX customer_authority_status
    ON erpx_customer_authority_write.account_guards
        (customer_status, updated_at DESC, customer_id DESC);

CREATE TABLE erpx_customer_authority_write.authority_link_guards (
    customer_id UUID NOT NULL,
    scope_key TEXT NOT NULL,
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('holding', 'company', 'branch')),
    holding_id UUID NOT NULL,
    company_id UUID,
    branch_id UUID,
    is_primary BOOLEAN NOT NULL DEFAULT FALSE,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (customer_id, scope_key),
    FOREIGN KEY (customer_id)
        REFERENCES erpx_customer_authority_write.account_guards (customer_id)
        ON DELETE CASCADE,
    CHECK (
        (scope_kind = 'holding' AND company_id IS NULL AND branch_id IS NULL)
        OR (scope_kind = 'company' AND company_id IS NOT NULL AND branch_id IS NULL)
        OR (scope_kind = 'branch' AND company_id IS NOT NULL AND branch_id IS NOT NULL)
    )
);

CREATE UNIQUE INDEX customer_authority_one_primary
    ON erpx_customer_authority_write.authority_link_guards (customer_id)
    WHERE is_primary;

CREATE INDEX customer_authority_scope
    ON erpx_customer_authority_write.authority_link_guards
        (scope_key, customer_id);

-- D13: normalized Platform governance authority. These records govern
-- operations and evidence; migrator/deployer executors remain separate ports.
CREATE TABLE erpx_platform_administration_write.governance_records (
    record_id UUID PRIMARY KEY,
    record_version BIGINT NOT NULL CHECK (record_version > 0),
    record_kind TEXT NOT NULL CHECK (record_kind IN (
        'data_plane_operation', 'migration', 'release'
    )),
    record_scope JSONB NOT NULL,
    record_title TEXT NOT NULL,
    subject_ref TEXT NOT NULL,
    evidence_ref TEXT,
    record_status TEXT NOT NULL CHECK (record_status IN (
        'requested', 'provisioning', 'verified', 'active', 'suspended',
        'proposed', 'approved', 'rejected', 'running', 'draft', 'ready',
        'deploying', 'verifying', 'completed', 'released', 'failed',
        'rolled_back', 'retired', 'cancelled'
    )),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX platform_governance_kind_state
    ON erpx_platform_administration_write.governance_records
        (record_kind, record_status, updated_at DESC, record_id DESC);

CREATE TABLE erpx_platform_administration_write.governance_record_receipts (
    idempotency_key TEXT PRIMARY KEY,
    command_id UUID NOT NULL UNIQUE,
    request_hash BYTEA NOT NULL CHECK (octet_length(request_hash) = 32),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version > 0),
    correlation_id UUID NOT NULL,
    receipt JSONB NOT NULL,
    committed_at TIMESTAMPTZ NOT NULL,
    FOREIGN KEY (record_id)
        REFERENCES erpx_platform_administration_write.governance_records (record_id)
);

CREATE TABLE erpx_platform_administration_write.governance_record_audit (
    audit_id UUID PRIMARY KEY,
    command_id UUID NOT NULL UNIQUE,
    correlation_id UUID NOT NULL,
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version > 0),
    record_kind TEXT NOT NULL CHECK (record_kind IN (
        'data_plane_operation', 'migration', 'release'
    )),
    record_status TEXT NOT NULL,
    subject_ref TEXT NOT NULL,
    evidence_ref TEXT,
    actor JSONB NOT NULL,
    audit_context JSONB NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('create', 'transition')),
    occurred_at TIMESTAMPTZ NOT NULL,
    FOREIGN KEY (record_id)
        REFERENCES erpx_platform_administration_write.governance_records (record_id)
);


CREATE SCHEMA IF NOT EXISTS erpx_web;

CREATE TABLE erpx_web.session_bundles (
    id UUID PRIMARY KEY,
    selector_digest BYTEA NOT NULL UNIQUE CHECK (octet_length(selector_digest) = 32),
    active_session_id UUID NOT NULL,
    version BIGINT NOT NULL CHECK (version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL CHECK (updated_at >= created_at),
    expires_at TIMESTAMPTZ NOT NULL CHECK (expires_at > updated_at)
);

CREATE INDEX web_session_bundles_expiry_idx
    ON erpx_web.session_bundles (expires_at);

CREATE TABLE erpx_web.session_accounts (
    bundle_id UUID NOT NULL REFERENCES erpx_web.session_bundles(id) ON DELETE CASCADE,
    session_id UUID NOT NULL,
    principal_ref TEXT NOT NULL CHECK (
        principal_ref = btrim(principal_ref)
        AND length(principal_ref) BETWEEN 1 AND 256
    ),
    key_reference TEXT NOT NULL CHECK (length(key_reference) BETWEEN 1 AND 128),
    access_nonce BYTEA NOT NULL CHECK (octet_length(access_nonce) = 12),
    access_ciphertext BYTEA NOT NULL CHECK (octet_length(access_ciphertext) > 16),
    refresh_nonce BYTEA NOT NULL CHECK (octet_length(refresh_nonce) = 12),
    refresh_ciphertext BYTEA NOT NULL CHECK (octet_length(refresh_ciphertext) > 16),
    access_expires_at TIMESTAMPTZ NOT NULL,
    refresh_expires_at TIMESTAMPTZ NOT NULL CHECK (refresh_expires_at > access_expires_at),
    last_used_at TIMESTAMPTZ NOT NULL CHECK (last_used_at <= refresh_expires_at),
    mru_position SMALLINT NOT NULL CHECK (mru_position BETWEEN 0 AND 4),
    PRIMARY KEY (bundle_id, session_id),
    UNIQUE (bundle_id, principal_ref),
    UNIQUE (bundle_id, mru_position)
);

ALTER TABLE erpx_web.session_bundles
    ADD CONSTRAINT web_session_active_account_fk
    FOREIGN KEY (id, active_session_id)
    REFERENCES erpx_web.session_accounts(bundle_id, session_id)
    DEFERRABLE INITIALLY DEFERRED;

REVOKE ALL ON SCHEMA erpx_web FROM PUBLIC;
REVOKE ALL ON ALL TABLES IN SCHEMA erpx_web FROM PUBLIC;


ALTER TABLE erpx_web.session_accounts
    ADD COLUMN active_role_source TEXT,
    ADD COLUMN active_role_selection_id TEXT,
    ADD COLUMN active_role_key TEXT,
    ADD COLUMN active_scope_key TEXT,
    ADD COLUMN authorization_context_version BIGINT;

ALTER TABLE erpx_web.session_accounts
    ADD CONSTRAINT web_session_role_pair_check CHECK (
        (active_role_source IS NULL AND active_role_selection_id IS NULL)
        OR (
            active_role_source IN ('local', 'directory')
            AND length(active_role_selection_id) BETWEEN 1 AND 256
        )
    ),
    ADD CONSTRAINT web_session_role_labels_pair_check CHECK (
        (active_role_key IS NULL AND active_scope_key IS NULL)
        OR (
            length(active_role_key) BETWEEN 1 AND 256
            AND length(active_scope_key) BETWEEN 1 AND 256
        )
    ),
    ADD CONSTRAINT web_session_auth_context_version_check CHECK (
        authorization_context_version IS NULL OR authorization_context_version > 0
    );


ALTER TABLE erpx_web.session_accounts
    ADD COLUMN development_context_nonce BYTEA,
    ADD COLUMN development_context_ciphertext BYTEA;

ALTER TABLE erpx_web.session_accounts
    ADD CONSTRAINT web_session_development_context_pair_check CHECK (
        (development_context_nonce IS NULL AND development_context_ciphertext IS NULL)
        OR (
            octet_length(development_context_nonce) = 12
            AND octet_length(development_context_ciphertext) > 16
        )
    );


ALTER TABLE erpx_web.session_accounts
    ADD COLUMN development_role_source TEXT,
    ADD COLUMN development_role_selection_id TEXT,
    ADD COLUMN development_role_key TEXT,
    ADD COLUMN development_scope_key TEXT;

ALTER TABLE erpx_web.session_accounts
    ADD CONSTRAINT web_session_development_role_pair_check CHECK (
        (development_role_source IS NULL AND development_role_selection_id IS NULL)
        OR (
            development_role_source IN ('local', 'directory')
            AND length(development_role_selection_id) BETWEEN 1 AND 256
        )
    ),
    ADD CONSTRAINT web_session_development_role_labels_pair_check CHECK (
        (development_role_key IS NULL AND development_scope_key IS NULL)
        OR (
            length(development_role_key) BETWEEN 1 AND 256
            AND length(development_scope_key) BETWEEN 1 AND 256
        )
    ),
    ADD CONSTRAINT web_session_development_role_requires_context_check CHECK (
        development_role_source IS NULL
        OR development_context_nonce IS NOT NULL
    );


-- Clean-rebuild generation marker. The Rust migration entry point rejects
-- non-empty databases without this marker before this migration can run.
ALTER TABLE erpx_cqrs_write.outbox_events
    ADD COLUMN claim_token UUID,
    ADD COLUMN claim_until TIMESTAMPTZ,
    ADD COLUMN dead_lettered_at TIMESTAMPTZ,
    ADD CONSTRAINT cqrs_control_outbox_claim_consistency
        CHECK ((claim_token IS NULL) = (claim_until IS NULL)),
    ADD CONSTRAINT cqrs_control_outbox_terminal_consistency
        CHECK (published_at IS NULL OR dead_lettered_at IS NULL);

CREATE INDEX cqrs_control_outbox_expired_claims
    ON erpx_cqrs_write.outbox_events (claim_until, global_sequence)
    WHERE published_at IS NULL AND dead_lettered_at IS NULL;

CREATE INDEX cqrs_control_outbox_due_v2
    ON erpx_cqrs_write.outbox_events (available_at, global_sequence)
    WHERE published_at IS NULL AND dead_lettered_at IS NULL;

CREATE SCHEMA erpx_meta;

CREATE TABLE erpx_meta.schema_manifest (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    schema_revision SMALLINT NOT NULL CHECK (schema_revision = 2),
    manifest_version SMALLINT NOT NULL CHECK (manifest_version = 1),
    database_role TEXT NOT NULL CHECK (database_role = 'control_write'),
    migration_set TEXT NOT NULL CHECK (migration_set = 'control-write'),
    installed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO erpx_meta.schema_manifest
    (singleton, schema_revision, manifest_version, database_role, migration_set)
VALUES (TRUE, 2, 1, 'control_write', 'control-write');


-- Typed relational authority for current control-plane aggregates. CQRS
-- receipts, audit and outbox rows remain durable facts, but aggregate JSON is
-- not a command rehydration source.

ALTER TABLE erpx_commercial_write.catalog_plan_guards
    ADD COLUMN display_name TEXT NOT NULL,
    ADD COLUMN amount_minor BIGINT NOT NULL CHECK (amount_minor > 0),
    ADD COLUMN currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    ADD COLUMN created_at TIMESTAMPTZ NOT NULL,
    ADD CONSTRAINT commercial_catalog_plan_time CHECK (updated_at >= created_at);

ALTER TABLE erpx_platform_administration_write.holding_guards
    ADD COLUMN created_at TIMESTAMPTZ NOT NULL,
    ADD CONSTRAINT platform_holding_time CHECK (updated_at >= created_at);

ALTER TABLE erpx_platform_administration_write.connector_guards
    DROP COLUMN capabilities,
    ADD COLUMN scope_kind TEXT NOT NULL CHECK (scope_kind IN (
        'platform','holding','company','branch'
    )),
    ADD COLUMN holding_id UUID,
    ADD COLUMN company_id UUID,
    ADD COLUMN branch_id UUID,
    ADD COLUMN connector_kind_type TEXT NOT NULL CHECK (connector_kind_type IN (
        'stripe','minio','n8n','ollama','other'
    )),
    ADD COLUMN connector_kind_other TEXT,
    ADD COLUMN readiness_evidence_id TEXT,
    ADD COLUMN readiness_configuration_digest BYTEA CHECK (
        readiness_configuration_digest IS NULL
        OR octet_length(readiness_configuration_digest) = 32
    ),
    ADD COLUMN readiness_ready BOOLEAN,
    ADD COLUMN readiness_failure_category TEXT,
    ADD COLUMN readiness_observed_at TIMESTAMPTZ,
    ADD COLUMN created_at TIMESTAMPTZ NOT NULL,
    ADD CONSTRAINT platform_connector_scope_shape CHECK (
        (scope_kind='platform' AND holding_id IS NULL AND company_id IS NULL AND branch_id IS NULL)
        OR (scope_kind='holding' AND holding_id IS NOT NULL AND company_id IS NULL AND branch_id IS NULL)
        OR (scope_kind='company' AND holding_id IS NOT NULL AND company_id IS NOT NULL AND branch_id IS NULL)
        OR (scope_kind='branch' AND holding_id IS NOT NULL AND company_id IS NOT NULL AND branch_id IS NOT NULL)
    ),
    ADD CONSTRAINT platform_connector_kind_shape CHECK (
        (connector_kind_type='other' AND connector_kind_other IS NOT NULL)
        OR (connector_kind_type<>'other' AND connector_kind_other IS NULL)
    ),
    ADD CONSTRAINT platform_connector_readiness_shape CHECK (
        (readiness_evidence_id IS NULL
            AND readiness_configuration_digest IS NULL
            AND readiness_ready IS NULL
            AND readiness_failure_category IS NULL
            AND readiness_observed_at IS NULL)
        OR (readiness_evidence_id IS NOT NULL
            AND readiness_configuration_digest IS NOT NULL
            AND readiness_ready IS NOT NULL
            AND readiness_observed_at IS NOT NULL
            AND (readiness_ready = (readiness_failure_category IS NULL)))
    ),
    ADD CONSTRAINT platform_connector_time CHECK (updated_at >= created_at);

CREATE TABLE erpx_platform_administration_write.connector_capabilities (
    connector_id UUID NOT NULL,
    capability TEXT NOT NULL,
    PRIMARY KEY (connector_id, capability),
    FOREIGN KEY (connector_id)
        REFERENCES erpx_platform_administration_write.connector_guards(connector_id)
        ON DELETE CASCADE
);

CREATE TABLE erpx_platform_administration_write.connector_readiness_capabilities (
    connector_id UUID NOT NULL,
    capability TEXT NOT NULL,
    PRIMARY KEY (connector_id, capability),
    FOREIGN KEY (connector_id)
        REFERENCES erpx_platform_administration_write.connector_guards(connector_id)
        ON DELETE CASCADE
);

ALTER TABLE erpx_customer_authority_write.account_guards
    ADD COLUMN trading_name TEXT NOT NULL,
    ADD COLUMN legal_name TEXT,
    ADD COLUMN registration_number TEXT,
    ADD COLUMN tax_identifier TEXT,
    ADD COLUMN country TEXT NOT NULL CHECK (country ~ '^[A-Z]{2}$'),
    ADD COLUMN created_at TIMESTAMPTZ NOT NULL,
    ADD CONSTRAINT customer_authority_account_time CHECK (updated_at >= created_at);

ALTER TABLE erpx_brand_write.profile_guards
    ADD COLUMN scope_kind TEXT NOT NULL CHECK (scope_kind IN (
        'platform','holding','business_unit'
    )),
    ADD COLUMN data_profile_key TEXT,
    ADD COLUMN holding_id UUID,
    ADD COLUMN business_unit TEXT CHECK (business_unit IN (
        'operations','finance','sales','procurement','inventory','assets',
        'maintenance','quality','marketing-crm','hr-payroll','point-of-sale','legal'
    )),
    ADD COLUMN created_at TIMESTAMPTZ NOT NULL,
    ADD CONSTRAINT brand_profile_scope_shape CHECK (
        (scope_kind='platform' AND data_profile_key IS NULL AND holding_id IS NULL AND business_unit IS NULL)
        OR (scope_kind='holding' AND data_profile_key IS NOT NULL AND holding_id IS NOT NULL AND business_unit IS NULL)
        OR (scope_kind='business_unit' AND data_profile_key IS NOT NULL AND holding_id IS NOT NULL AND business_unit IS NOT NULL)
    ),
    ADD CONSTRAINT brand_profile_time CHECK (updated_at >= created_at);

ALTER TABLE erpx_brand_write.profile_revisions
    DROP COLUMN definition,
    ADD COLUMN updated_at TIMESTAMPTZ NOT NULL,
    ADD CONSTRAINT brand_revision_time CHECK (updated_at >= created_at);

CREATE TABLE erpx_brand_write.profile_revision_tokens (
    profile_id UUID NOT NULL,
    revision BIGINT NOT NULL CHECK (revision > 0),
    display_name TEXT NOT NULL,
    short_name TEXT NOT NULL,
    light_primary TEXT NOT NULL, light_on_primary TEXT NOT NULL,
    light_accent TEXT NOT NULL, light_on_accent TEXT NOT NULL,
    light_canvas TEXT NOT NULL, light_surface TEXT NOT NULL,
    light_surface_raised TEXT NOT NULL, light_border TEXT NOT NULL,
    light_text TEXT NOT NULL, light_text_muted TEXT NOT NULL,
    light_link TEXT NOT NULL, light_focus TEXT NOT NULL,
    dark_primary TEXT NOT NULL, dark_on_primary TEXT NOT NULL,
    dark_accent TEXT NOT NULL, dark_on_accent TEXT NOT NULL,
    dark_canvas TEXT NOT NULL, dark_surface TEXT NOT NULL,
    dark_surface_raised TEXT NOT NULL, dark_border TEXT NOT NULL,
    dark_text TEXT NOT NULL, dark_text_muted TEXT NOT NULL,
    dark_link TEXT NOT NULL, dark_focus TEXT NOT NULL,
    font_pack TEXT NOT NULL CHECK (font_pack IN (
        'erpx_sans','corporate_sans','thai_formal','system_ui'
    )),
    heading_scale_percent INTEGER NOT NULL CHECK (heading_scale_percent BETWEEN 90 AND 125),
    body_scale_percent INTEGER NOT NULL CHECK (body_scale_percent BETWEEN 90 AND 120),
    line_height_percent INTEGER NOT NULL CHECK (line_height_percent BETWEEN 120 AND 180),
    density TEXT NOT NULL CHECK (density IN ('compact','comfortable','spacious')),
    radius TEXT NOT NULL CHECK (radius IN ('square','soft','rounded')),
    elevation TEXT NOT NULL CHECK (elevation IN ('flat','subtle','elevated')),
    navigation TEXT NOT NULL CHECK (navigation IN ('solid','tinted')),
    navigation_expanded_by_default BOOLEAN NOT NULL,
    motion TEXT NOT NULL CHECK (motion IN ('none','subtle','standard')),
    PRIMARY KEY (profile_id, revision),
    FOREIGN KEY (profile_id, revision)
        REFERENCES erpx_brand_write.profile_revisions(profile_id, revision)
        ON DELETE CASCADE
);

CREATE TABLE erpx_brand_write.profile_revision_categories (
    profile_id UUID NOT NULL,
    revision BIGINT NOT NULL CHECK (revision > 0),
    category TEXT NOT NULL CHECK (category IN (
        'identity','color','typography','shape','elevation','density','navigation','motion'
    )),
    category_role TEXT NOT NULL CHECK (category_role IN ('overridden','locked')),
    PRIMARY KEY (profile_id, revision, category, category_role),
    FOREIGN KEY (profile_id, revision)
        REFERENCES erpx_brand_write.profile_revisions(profile_id, revision)
        ON DELETE CASCADE
);

CREATE TABLE erpx_brand_write.profile_revision_assets (
    profile_id UUID NOT NULL,
    revision BIGINT NOT NULL CHECK (revision > 0),
    asset_kind TEXT NOT NULL CHECK (asset_kind IN (
        'logo_mark','wordmark','favicon','login_artwork'
    )),
    object_key TEXT NOT NULL,
    sha256 TEXT NOT NULL CHECK (sha256 ~ '^[0-9a-f]{64}$'),
    media_type TEXT NOT NULL,
    byte_length BIGINT NOT NULL CHECK (byte_length > 0),
    width BIGINT CHECK (width BETWEEN 1 AND 4096),
    height BIGINT CHECK (height BETWEEN 1 AND 4096),
    PRIMARY KEY (profile_id, revision, asset_kind),
    FOREIGN KEY (profile_id, revision)
        REFERENCES erpx_brand_write.profile_revisions(profile_id, revision)
        ON DELETE CASCADE
);

-- PLATFORM_COMPLETION_TYPED_AUTHORITY_BEGIN
-- Active platform-completion commands use the existing holding registry,
-- backup-job and projection-switch rows as their typed lifecycle authority.
-- Reconciliation and connector roots receive dedicated scalar/child tables.
-- No serialized aggregate document is retained or consulted.
ALTER TABLE erpx_control.holding_registry
    ADD COLUMN lifecycle_aggregate_version BIGINT
        CHECK (lifecycle_aggregate_version IS NULL OR lifecycle_aggregate_version > 0);

ALTER TABLE erpx_control.holding_backup_jobs
    ADD COLUMN aggregate_version BIGINT NOT NULL DEFAULT 1
        CHECK (aggregate_version > 0),
    ADD CONSTRAINT holding_backup_jobs_holding_holding_fk
        FOREIGN KEY (holding_id, data_profile_key)
        REFERENCES erpx_control.holding_registry(holding_id, data_profile_key)
        ON DELETE RESTRICT;

ALTER TABLE erpx_control.holding_projection_switches
    ADD COLUMN aggregate_version BIGINT NOT NULL DEFAULT 1
        CHECK (aggregate_version > 0);

CREATE SCHEMA erpx_platform_completion_write;

CREATE TABLE erpx_platform_completion_write.reconciliation_authority (
    aggregate_id UUID PRIMARY KEY,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    report_id UUID NOT NULL UNIQUE,
    holding_id TEXT NOT NULL,
    data_profile_key TEXT NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL,
    reconciliation_state TEXT NOT NULL CHECK (
        reconciliation_state IN ('healthy','drifted','unavailable')
    ),
    registry_state TEXT NOT NULL CHECK (
        registry_state IN ('provisioning','active','suspended','recovery')
    ),
    provisioning_state TEXT NOT NULL CHECK (
        provisioning_state IN ('missing','provisioning','active','suspended','recovery','failed')
    ),
    write_healthy BOOLEAN NOT NULL,
    read_healthy BOOLEAN NOT NULL,
    storage_healthy BOOLEAN NOT NULL,
    replacement_read_state TEXT CHECK (
        replacement_read_state IS NULL OR replacement_read_state IN (
            'rebuilding','ready','failed','activated'
        )
    ),
    latest_backup_verified BOOLEAN NOT NULL,
    connector_count INTEGER NOT NULL CHECK (connector_count >= 0),
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE (holding_id),
    FOREIGN KEY (holding_id, data_profile_key)
        REFERENCES erpx_control.holding_registry(holding_id, data_profile_key)
        ON DELETE RESTRICT,
    CHECK (updated_at >= observed_at)
);

CREATE TABLE erpx_platform_completion_write.reconciliation_drift_codes (
    aggregate_id UUID NOT NULL,
    drift_code TEXT NOT NULL CHECK (octet_length(drift_code) BETWEEN 1 AND 120),
    PRIMARY KEY (aggregate_id, drift_code),
    FOREIGN KEY (aggregate_id)
        REFERENCES erpx_platform_completion_write.reconciliation_authority(aggregate_id)
        ON DELETE CASCADE
);

CREATE TABLE erpx_platform_completion_write.connector_authority (
    connector_id UUID PRIMARY KEY,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    connector_version BIGINT NOT NULL CHECK (connector_version > 0),
    connector_kind TEXT NOT NULL CHECK (connector_kind IN (
        'stripe','marketing_provider','finance_bank','generic_ldap',
        'active_directory','univention_corporate_server'
    )),
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('platform','holding')),
    holding_id TEXT,
    data_profile_key TEXT,
    provider_key TEXT NOT NULL CHECK (octet_length(provider_key) BETWEEN 1 AND 120),
    credential_ref TEXT NOT NULL CHECK (
        octet_length(credential_ref) BETWEEN 1 AND 200
        AND position('://' IN credential_ref)=0
        AND position('=' IN credential_ref)=0
    ),
    configuration_digest TEXT NOT NULL CHECK (configuration_digest ~ '^[0-9a-f]{64}$'),
    connector_state TEXT NOT NULL CHECK (
        connector_state IN ('pending','active','suspended','revoked')
    ),
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE (connector_id, connector_version),
    FOREIGN KEY (holding_id, data_profile_key)
        REFERENCES erpx_control.holding_registry(holding_id, data_profile_key)
        ON DELETE RESTRICT,
    CHECK (
        (scope_kind='platform' AND holding_id IS NULL AND data_profile_key IS NULL)
        OR (scope_kind='holding' AND holding_id IS NOT NULL AND data_profile_key IS NOT NULL)
    ),
    CHECK (connector_version=aggregate_version)
);

CREATE INDEX platform_completion_connector_scope
    ON erpx_platform_completion_write.connector_authority
        (scope_kind, holding_id, data_profile_key, connector_state, connector_id);

CREATE TABLE erpx_platform_completion_write.connector_capabilities (
    connector_id UUID NOT NULL,
    capability TEXT NOT NULL CHECK (octet_length(capability) BETWEEN 1 AND 120),
    PRIMARY KEY (connector_id, capability),
    FOREIGN KEY (connector_id)
        REFERENCES erpx_platform_completion_write.connector_authority(connector_id)
        ON DELETE CASCADE
);
-- PLATFORM_COMPLETION_TYPED_AUTHORITY_END

-- HOLDING_BACKUP_JOB_TYPED_VERSION_AUTHORITY
-- The typed holding_backup_jobs row owns lifecycle state and version. Outbox
-- payloads are evidence only; no generic control aggregate snapshot remains.
ALTER TABLE erpx_control.holding_backup_jobs
    ADD COLUMN IF NOT EXISTS aggregate_version BIGINT NOT NULL DEFAULT 0
        CHECK (aggregate_version >= 0);


-- Hard cutover: aggregate state is owned by context-specific typed relational
-- tables. Remove the obsolete shared JSON snapshot authority for databases
-- that applied an earlier current foundation revision.
DROP TABLE IF EXISTS erpx_cqrs_write.aggregate_snapshots;
DROP TABLE IF EXISTS erpx_control.aggregate_snapshots;

-- Final control_write / control-write generation fence. The authoritative
-- erpx_meta.schema_manifest must still identify schema_revision 2.
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM erpx_meta.schema_manifest
        WHERE singleton=TRUE AND schema_revision=2 AND manifest_version=1
          AND database_role='control_write' AND migration_set='control-write'
    ) THEN
        RAISE EXCEPTION 'control-write current manifest is not current';
    END IF;
END;
$$;


-- Retire the n8n connector kind without rewriting durable authority or CQRS
-- evidence. Any legacy row aborts the migration so an operator must reconcile
-- it explicitly before this generation can advance.
DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM erpx_platform_administration_write.connector_guards
        WHERE lower(connector_kind_type) = 'n8n'
           OR lower(coalesce(connector_kind_other, '')) = 'n8n'
    ) OR EXISTS (
        SELECT 1 FROM erpx_cqrs_write.command_receipts
        WHERE context_name = 'platform_administration'
          AND lower(coalesce(receipt::text, '')) LIKE '%n8n%'
    ) OR EXISTS (
        SELECT 1 FROM erpx_cqrs_write.audit_log
        WHERE context_name = 'platform_administration'
          AND lower(details::text) LIKE '%n8n%'
    ) OR EXISTS (
        SELECT 1 FROM erpx_cqrs_write.outbox_events
        WHERE context_name = 'platform_administration'
          AND lower(payload::text) LIKE '%n8n%'
    ) THEN
        RAISE EXCEPTION 'n8n connector evidence exists; retirement requires explicit reconciliation';
    END IF;
END;
$$;

ALTER TABLE erpx_platform_administration_write.connector_guards
    DROP CONSTRAINT connector_guards_connector_kind_type_check,
    ADD CONSTRAINT connector_guards_connector_kind_type_check CHECK (
        connector_kind_type IN ('stripe','minio','ollama','other')
    ),
    ADD CONSTRAINT platform_connector_other_kind_retirement CHECK (
        connector_kind_other IS NULL OR lower(connector_kind_other) <> 'n8n'
    );

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM erpx_meta.schema_manifest
        WHERE singleton=TRUE AND schema_revision=2 AND manifest_version=1
          AND database_role='control_write' AND migration_set='control-write'
    ) THEN
        RAISE EXCEPTION 'control-write current manifest is not current';
    END IF;
END;
$$;
