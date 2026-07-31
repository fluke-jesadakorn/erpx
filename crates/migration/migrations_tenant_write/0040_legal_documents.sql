-- Typed, holding-scoped legal and document source state.  Raw document
-- content, credentials, and transport/provider state are intentionally not
-- represented in this migration; object_ref is an opaque bounded reference.

CREATE TABLE IF NOT EXISTS erpx_tenant.legal_command_idempotency (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> '' AND octet_length(holding_ref) <= 255),
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> '' AND octet_length(idempotency_key) <= 512),
    request_hash TEXT NOT NULL CHECK (request_hash ~ '^[0-9a-f]{64}$'),
    aggregate_type TEXT NOT NULL CHECK (aggregate_type IN ('legal_contract', 'legal_document', 'legal_access_history')),
    aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    event_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, idempotency_key)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.legal_contracts (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> '' AND octet_length(holding_ref) <= 255),
    contract_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    reference TEXT NOT NULL CHECK (btrim(reference) <> '' AND octet_length(reference) <= 255),
    title TEXT NOT NULL CHECK (btrim(title) <> '' AND octet_length(title) <= 255),
    object_ref TEXT NOT NULL CHECK (btrim(object_ref) <> '' AND octet_length(object_ref) <= 255),
    content_sha256 TEXT NOT NULL CHECK (content_sha256 ~ '^sha256:[0-9A-Fa-f]{64}$'),
    retention_until TIMESTAMPTZ NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('draft', 'review', 'approved', 'active', 'expired', 'terminated', 'archived')),
    created_by TEXT NOT NULL CHECK (btrim(created_by) <> '' AND octet_length(created_by) <= 255),
    approved_by TEXT CHECK (approved_by IS NULL OR (btrim(approved_by) <> '' AND octet_length(approved_by) <= 255)),
    version BIGINT NOT NULL CHECK (version > 0),
    last_command_id UUID NOT NULL,
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> '' AND octet_length(idempotency_key) <= 512),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, contract_id),
    UNIQUE (holding_ref, reference),
    CHECK (approved_by IS NULL OR approved_by <> created_by),
    CHECK ((status IN ('draft', 'review') AND approved_by IS NULL)
        OR (status IN ('approved', 'active', 'expired', 'terminated', 'archived') AND approved_by IS NOT NULL))
);

CREATE TABLE IF NOT EXISTS erpx_tenant.legal_contract_parties (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> '' AND octet_length(holding_ref) <= 255),
    contract_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    party_id UUID NOT NULL,
    party_role TEXT NOT NULL CHECK (btrim(party_role) <> '' AND octet_length(party_role) <= 255),
    PRIMARY KEY (holding_ref, contract_id, party_id),
    FOREIGN KEY (holding_ref, contract_id)
        REFERENCES erpx_tenant.legal_contracts (holding_ref, contract_id)
        ON DELETE RESTRICT,
    UNIQUE (holding_ref, contract_id, party_id, party_role)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.legal_contract_approvals (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> '' AND octet_length(holding_ref) <= 255),
    contract_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    approved_by TEXT NOT NULL CHECK (btrim(approved_by) <> '' AND octet_length(approved_by) <= 255),
    approved_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    contract_version BIGINT NOT NULL CHECK (contract_version > 0),
    PRIMARY KEY (holding_ref, contract_id),
    FOREIGN KEY (holding_ref, contract_id)
        REFERENCES erpx_tenant.legal_contracts (holding_ref, contract_id)
        ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS erpx_tenant.legal_contract_retention (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> '' AND octet_length(holding_ref) <= 255),
    contract_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    retention_until TIMESTAMPTZ NOT NULL,
    retention_state TEXT NOT NULL CHECK (retention_state IN ('active', 'ended')),
    contract_version BIGINT NOT NULL CHECK (contract_version > 0),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, contract_id),
    FOREIGN KEY (holding_ref, contract_id)
        REFERENCES erpx_tenant.legal_contracts (holding_ref, contract_id)
        ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS erpx_tenant.legal_documents (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> '' AND octet_length(holding_ref) <= 255),
    document_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    title TEXT NOT NULL CHECK (btrim(title) <> '' AND octet_length(title) <= 255),
    current_version BIGINT NOT NULL CHECK (current_version >= 0),
    archived BOOLEAN NOT NULL DEFAULT false,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> '' AND octet_length(idempotency_key) <= 512),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, document_id)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.legal_document_versions (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> '' AND octet_length(holding_ref) <= 255),
    document_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    version BIGINT NOT NULL CHECK (version > 0),
    object_ref TEXT NOT NULL CHECK (btrim(object_ref) <> '' AND octet_length(object_ref) <= 255),
    content_sha256 TEXT NOT NULL CHECK (content_sha256 ~ '^sha256:[0-9A-Fa-f]{64}$'),
    created_at TIMESTAMPTZ NOT NULL,
    created_by TEXT NOT NULL CHECK (btrim(created_by) <> '' AND octet_length(created_by) <= 255),
    PRIMARY KEY (holding_ref, document_id, version),
    FOREIGN KEY (holding_ref, document_id)
        REFERENCES erpx_tenant.legal_documents (holding_ref, document_id)
        ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS erpx_tenant.legal_access_history (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> '' AND octet_length(holding_ref) <= 255),
    entry_id UUID NOT NULL,
    document_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    document_version BIGINT NOT NULL CHECK (document_version > 0),
    actor_ref TEXT NOT NULL CHECK (btrim(actor_ref) <> '' AND octet_length(actor_ref) <= 255),
    action TEXT NOT NULL CHECK (btrim(action) <> '' AND octet_length(action) <= 255),
    purpose TEXT NOT NULL CHECK (btrim(purpose) <> '' AND octet_length(purpose) <= 512),
    observed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_ref, entry_id),
    FOREIGN KEY (holding_ref, document_id, document_version)
        REFERENCES erpx_tenant.legal_document_versions (holding_ref, document_id, version)
        ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS legal_contract_scope_idx
    ON erpx_tenant.legal_contracts (holding_ref, holding_id, company_id, branch_id, status, updated_at DESC);
CREATE INDEX IF NOT EXISTS legal_document_scope_idx
    ON erpx_tenant.legal_documents (holding_ref, holding_id, company_id, branch_id, archived, updated_at DESC);
CREATE INDEX IF NOT EXISTS legal_document_versions_scope_idx
    ON erpx_tenant.legal_document_versions (holding_ref, document_id, version);
CREATE INDEX IF NOT EXISTS legal_access_history_scope_idx
    ON erpx_tenant.legal_access_history (holding_ref, document_id, observed_at DESC, entry_id);
CREATE INDEX IF NOT EXISTS legal_idempotency_aggregate_idx
    ON erpx_tenant.legal_command_idempotency (holding_ref, aggregate_type, aggregate_id, aggregate_version);

ALTER TABLE erpx_tenant.legal_command_idempotency ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.legal_command_idempotency FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.legal_contracts ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.legal_contracts FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.legal_contract_parties ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.legal_contract_parties FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.legal_contract_approvals ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.legal_contract_approvals FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.legal_contract_retention ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.legal_contract_retention FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.legal_documents ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.legal_documents FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.legal_document_versions ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.legal_document_versions FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.legal_access_history ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.legal_access_history FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_legal_command_idempotency ON erpx_tenant.legal_command_idempotency;
CREATE POLICY tenant_scope_legal_command_idempotency ON erpx_tenant.legal_command_idempotency
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_legal_contracts ON erpx_tenant.legal_contracts;
CREATE POLICY tenant_scope_legal_contracts ON erpx_tenant.legal_contracts
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_legal_contract_parties ON erpx_tenant.legal_contract_parties;
CREATE POLICY tenant_scope_legal_contract_parties ON erpx_tenant.legal_contract_parties
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_legal_contract_approvals ON erpx_tenant.legal_contract_approvals;
CREATE POLICY tenant_scope_legal_contract_approvals ON erpx_tenant.legal_contract_approvals
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_legal_contract_retention ON erpx_tenant.legal_contract_retention;
CREATE POLICY tenant_scope_legal_contract_retention ON erpx_tenant.legal_contract_retention
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_legal_documents ON erpx_tenant.legal_documents;
CREATE POLICY tenant_scope_legal_documents ON erpx_tenant.legal_documents
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_legal_document_versions ON erpx_tenant.legal_document_versions;
CREATE POLICY tenant_scope_legal_document_versions ON erpx_tenant.legal_document_versions
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_legal_access_history ON erpx_tenant.legal_access_history;
CREATE POLICY tenant_scope_legal_access_history ON erpx_tenant.legal_access_history
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

CREATE OR REPLACE FUNCTION erpx_tenant.reject_archived_legal_contract_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'DELETE' OR OLD.status = 'archived' THEN
        RAISE EXCEPTION 'archived legal contracts are immutable';
    END IF;
    IF NEW.approved_by IS NOT NULL AND NEW.approved_by = NEW.created_by THEN
        RAISE EXCEPTION 'legal contract approval requires separation';
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS legal_contract_terminal_immutable ON erpx_tenant.legal_contracts;
CREATE TRIGGER legal_contract_terminal_immutable
    BEFORE UPDATE OR DELETE ON erpx_tenant.legal_contracts
    FOR EACH ROW EXECUTE FUNCTION erpx_tenant.reject_archived_legal_contract_mutation();

CREATE OR REPLACE FUNCTION erpx_tenant.reject_archived_legal_party_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    contract_status TEXT;
BEGIN
    SELECT status INTO contract_status
      FROM erpx_tenant.legal_contracts
     WHERE holding_ref = COALESCE(NEW.holding_ref, OLD.holding_ref)
       AND contract_id = COALESCE(NEW.contract_id, OLD.contract_id);
    IF contract_status IS DISTINCT FROM 'draft' THEN
        RAISE EXCEPTION 'legal contract parties are immutable outside draft status';
    END IF;
    RETURN COALESCE(NEW, OLD);
END;
$$;

DROP TRIGGER IF EXISTS legal_contract_parties_terminal_immutable ON erpx_tenant.legal_contract_parties;
CREATE TRIGGER legal_contract_parties_terminal_immutable
    BEFORE INSERT OR UPDATE OR DELETE ON erpx_tenant.legal_contract_parties
    FOR EACH ROW EXECUTE FUNCTION erpx_tenant.reject_archived_legal_party_mutation();

CREATE OR REPLACE FUNCTION erpx_tenant.reject_legal_approval_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'legal approvals are immutable';
END;
$$;

DROP TRIGGER IF EXISTS legal_contract_approvals_immutable ON erpx_tenant.legal_contract_approvals;
CREATE TRIGGER legal_contract_approvals_immutable
    BEFORE UPDATE OR DELETE ON erpx_tenant.legal_contract_approvals
    FOR EACH ROW EXECUTE FUNCTION erpx_tenant.reject_legal_approval_mutation();

CREATE OR REPLACE FUNCTION erpx_tenant.reject_legal_version_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'legal document versions are immutable';
END;
$$;

DROP TRIGGER IF EXISTS legal_document_versions_immutable ON erpx_tenant.legal_document_versions;
CREATE TRIGGER legal_document_versions_immutable
    BEFORE UPDATE OR DELETE ON erpx_tenant.legal_document_versions
    FOR EACH ROW EXECUTE FUNCTION erpx_tenant.reject_legal_version_mutation();

CREATE OR REPLACE FUNCTION erpx_tenant.reject_legal_access_history_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'legal access history is immutable';
END;
$$;

DROP TRIGGER IF EXISTS legal_access_history_immutable ON erpx_tenant.legal_access_history;
CREATE TRIGGER legal_access_history_immutable
    BEFORE UPDATE OR DELETE ON erpx_tenant.legal_access_history
    FOR EACH ROW EXECUTE FUNCTION erpx_tenant.reject_legal_access_history_mutation();
