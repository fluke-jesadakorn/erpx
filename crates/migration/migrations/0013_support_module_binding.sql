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
