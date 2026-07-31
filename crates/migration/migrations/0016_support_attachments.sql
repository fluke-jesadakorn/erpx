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
