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
    represented_tenant_user TEXT,
    mode TEXT NOT NULL CHECK (mode IN ('diagnostic_read', 'tenant_mutation', 'impersonation')),
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
        OR (mode IN ('tenant_mutation', 'impersonation')
            AND expires_at <= requested_at + interval '2 hours')
    ),
    CHECK ((mode = 'impersonation') = (represented_tenant_user IS NOT NULL)),
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
    mode TEXT NOT NULL CHECK (mode IN ('diagnostic_read', 'tenant_mutation', 'impersonation')),
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
       OR OLD.represented_tenant_user IS DISTINCT FROM NEW.represented_tenant_user
       OR OLD.mode IS DISTINCT FROM NEW.mode
       OR OLD.permissions IS DISTINCT FROM NEW.permissions
       OR OLD.scope IS DISTINCT FROM NEW.scope
       OR OLD.reason IS DISTINCT FROM NEW.reason
       OR OLD.requested_at IS DISTINCT FROM NEW.requested_at
       OR OLD.expires_at IS DISTINCT FROM NEW.expires_at
       OR OLD.payload->'ticket_id' IS DISTINCT FROM NEW.payload->'ticket_id'
       OR OLD.payload->'customer_account_id' IS DISTINCT FROM NEW.payload->'customer_account_id'
       OR OLD.payload->'agent_ref' IS DISTINCT FROM NEW.payload->'agent_ref'
       OR OLD.payload->'represented_tenant_user' IS DISTINCT FROM NEW.payload->'represented_tenant_user'
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
