-- Security-factor and device commands retain an append-only, secret-free
-- idempotency receipt in the identity owner's write database. One-time TOTP
-- and recovery material is deliberately absent from this table.
CREATE TABLE IF NOT EXISTS erpx_identity.security_command_audit (
    audit_reference UUID PRIMARY KEY,
    command_id UUID NOT NULL UNIQUE,
    principal_id UUID NOT NULL REFERENCES erpx_identity.principals(id),
    operation TEXT NOT NULL CHECK (
        operation IN ('device_revoke', 'recovery_codes_rotate', 'totp_remove')
    ),
    target_id UUID,
    idempotency_key TEXT NOT NULL CHECK (
        length(idempotency_key) BETWEEN 1 AND 128
    ),
    request_digest BYTEA NOT NULL CHECK (octet_length(request_digest) = 32),
    purpose TEXT NOT NULL CHECK (length(purpose) BETWEEN 1 AND 256),
    locale TEXT NOT NULL CHECK (locale IN ('en', 'th')),
    occurred_at TIMESTAMPTZ NOT NULL,
    UNIQUE (principal_id, operation, idempotency_key)
);

CREATE INDEX IF NOT EXISTS security_command_audit_principal_idx
    ON erpx_identity.security_command_audit
       (principal_id, occurred_at DESC, audit_reference DESC);

CREATE OR REPLACE FUNCTION erpx_identity.reject_security_command_audit_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'security command audit is append-only';
END;
$$;

DROP TRIGGER IF EXISTS security_command_audit_append_only
    ON erpx_identity.security_command_audit;
CREATE TRIGGER security_command_audit_append_only
    BEFORE UPDATE OR DELETE ON erpx_identity.security_command_audit
    FOR EACH ROW
    EXECUTE FUNCTION erpx_identity.reject_security_command_audit_mutation();
