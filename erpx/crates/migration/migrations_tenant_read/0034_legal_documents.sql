-- Rebuildable, typed Legal/document read projection for one physically
-- isolated holding.  The tenant-write Legal tables and outbox remain
-- authoritative.  No raw document bytes, JSON payloads, credentials, URLs,
-- tokens, or provider state are represented here.

CREATE SCHEMA IF NOT EXISTS erpx_tenant;

CREATE TABLE IF NOT EXISTS erpx_tenant.legal_contract_read_rows (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    contract_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    reference TEXT NOT NULL CHECK (btrim(reference) <> '' AND char_length(reference) <= 255),
    title TEXT NOT NULL CHECK (btrim(title) <> '' AND char_length(title) <= 255),
    object_ref TEXT NOT NULL CHECK (btrim(object_ref) <> '' AND char_length(object_ref) <= 255),
    content_sha256 TEXT NOT NULL CHECK (content_sha256 ~ '^sha256:[0-9A-Fa-f]{64}$'),
    retention_until TIMESTAMPTZ NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('draft', 'review', 'approved', 'active', 'expired', 'terminated', 'archived')),
    created_by TEXT NOT NULL CHECK (btrim(created_by) <> '' AND char_length(created_by) <= 255),
    approved_by TEXT CHECK (approved_by IS NULL OR (btrim(approved_by) <> '' AND char_length(approved_by) <= 255)),
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    source_event_id UUID NOT NULL,
    source_contract_version INTEGER NOT NULL CHECK (source_contract_version > 0),
    source_event_version INTEGER NOT NULL CHECK (source_event_version > 0),
    source_actor_ref TEXT NOT NULL CHECK (btrim(source_actor_ref) <> '' AND char_length(source_actor_ref) <= 255),
    source_actor_kind TEXT NOT NULL CHECK (source_actor_kind IN ('platform_user', 'tenant_user', 'service', 'system_worker')),
    source_correlation_id UUID NOT NULL,
    source_causation_id UUID,
    source_idempotency_key TEXT NOT NULL CHECK (btrim(source_idempotency_key) <> '' AND char_length(source_idempotency_key) <= 512),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, contract_id)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.legal_contract_party_read_rows (
    holding_ref TEXT NOT NULL,
    contract_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    party_id UUID NOT NULL,
    party_role TEXT NOT NULL CHECK (btrim(party_role) <> '' AND char_length(party_role) <= 255),
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    source_event_id UUID NOT NULL,
    source_contract_version INTEGER NOT NULL CHECK (source_contract_version > 0),
    source_event_version INTEGER NOT NULL CHECK (source_event_version > 0),
    source_actor_ref TEXT NOT NULL CHECK (btrim(source_actor_ref) <> '' AND char_length(source_actor_ref) <= 255),
    source_actor_kind TEXT NOT NULL CHECK (source_actor_kind IN ('platform_user', 'tenant_user', 'service', 'system_worker')),
    source_correlation_id UUID NOT NULL,
    source_causation_id UUID,
    source_idempotency_key TEXT NOT NULL CHECK (btrim(source_idempotency_key) <> '' AND char_length(source_idempotency_key) <= 512),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, contract_id, party_id)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.legal_contract_approval_read_rows (
    holding_ref TEXT NOT NULL,
    contract_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    approved_by TEXT NOT NULL CHECK (btrim(approved_by) <> '' AND char_length(approved_by) <= 255),
    approved_at TIMESTAMPTZ NOT NULL,
    contract_version BIGINT NOT NULL CHECK (contract_version > 0),
    source_event_id UUID NOT NULL,
    source_contract_version INTEGER NOT NULL CHECK (source_contract_version > 0),
    source_event_version INTEGER NOT NULL CHECK (source_event_version > 0),
    source_actor_ref TEXT NOT NULL CHECK (btrim(source_actor_ref) <> '' AND char_length(source_actor_ref) <= 255),
    source_actor_kind TEXT NOT NULL CHECK (source_actor_kind IN ('platform_user', 'tenant_user', 'service', 'system_worker')),
    source_correlation_id UUID NOT NULL,
    source_causation_id UUID,
    source_idempotency_key TEXT NOT NULL CHECK (btrim(source_idempotency_key) <> '' AND char_length(source_idempotency_key) <= 512),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, contract_id)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.legal_contract_retention_read_rows (
    holding_ref TEXT NOT NULL,
    contract_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    retention_until TIMESTAMPTZ NOT NULL,
    retention_state TEXT NOT NULL CHECK (retention_state IN ('active', 'ended')),
    contract_version BIGINT NOT NULL CHECK (contract_version > 0),
    source_event_id UUID NOT NULL,
    source_contract_version INTEGER NOT NULL CHECK (source_contract_version > 0),
    source_event_version INTEGER NOT NULL CHECK (source_event_version > 0),
    source_actor_ref TEXT NOT NULL CHECK (btrim(source_actor_ref) <> '' AND char_length(source_actor_ref) <= 255),
    source_actor_kind TEXT NOT NULL CHECK (source_actor_kind IN ('platform_user', 'tenant_user', 'service', 'system_worker')),
    source_correlation_id UUID NOT NULL,
    source_causation_id UUID,
    source_idempotency_key TEXT NOT NULL CHECK (btrim(source_idempotency_key) <> '' AND char_length(source_idempotency_key) <= 512),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, contract_id)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.legal_document_read_rows (
    holding_ref TEXT NOT NULL,
    document_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    title TEXT NOT NULL CHECK (btrim(title) <> '' AND char_length(title) <= 255),
    current_version BIGINT NOT NULL CHECK (current_version >= 0),
    archived BOOLEAN NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    source_event_id UUID NOT NULL,
    source_contract_version INTEGER NOT NULL CHECK (source_contract_version > 0),
    source_event_version INTEGER NOT NULL CHECK (source_event_version > 0),
    source_actor_ref TEXT NOT NULL CHECK (btrim(source_actor_ref) <> '' AND char_length(source_actor_ref) <= 255),
    source_actor_kind TEXT NOT NULL CHECK (source_actor_kind IN ('platform_user', 'tenant_user', 'service', 'system_worker')),
    source_correlation_id UUID NOT NULL,
    source_causation_id UUID,
    source_idempotency_key TEXT NOT NULL CHECK (btrim(source_idempotency_key) <> '' AND char_length(source_idempotency_key) <= 512),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, document_id)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.legal_document_version_read_rows (
    holding_ref TEXT NOT NULL,
    document_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    version BIGINT NOT NULL CHECK (version > 0),
    object_ref TEXT NOT NULL CHECK (btrim(object_ref) <> '' AND char_length(object_ref) <= 255),
    content_sha256 TEXT NOT NULL CHECK (content_sha256 ~ '^sha256:[0-9A-Fa-f]{64}$'),
    created_at TIMESTAMPTZ NOT NULL,
    created_by TEXT NOT NULL CHECK (btrim(created_by) <> '' AND char_length(created_by) <= 255),
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    source_event_id UUID NOT NULL,
    source_contract_version INTEGER NOT NULL CHECK (source_contract_version > 0),
    source_event_version INTEGER NOT NULL CHECK (source_event_version > 0),
    source_actor_ref TEXT NOT NULL CHECK (btrim(source_actor_ref) <> '' AND char_length(source_actor_ref) <= 255),
    source_actor_kind TEXT NOT NULL CHECK (source_actor_kind IN ('platform_user', 'tenant_user', 'service', 'system_worker')),
    source_correlation_id UUID NOT NULL,
    source_causation_id UUID,
    source_idempotency_key TEXT NOT NULL CHECK (btrim(source_idempotency_key) <> '' AND char_length(source_idempotency_key) <= 512),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, document_id, version)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.legal_access_history_read_rows (
    holding_ref TEXT NOT NULL,
    entry_id UUID NOT NULL,
    document_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    document_version BIGINT NOT NULL CHECK (document_version > 0),
    actor_ref TEXT NOT NULL CHECK (btrim(actor_ref) <> '' AND char_length(actor_ref) <= 255),
    action TEXT NOT NULL CHECK (btrim(action) <> '' AND char_length(action) <= 255),
    purpose TEXT NOT NULL CHECK (btrim(purpose) <> '' AND char_length(purpose) <= 512),
    observed_at TIMESTAMPTZ NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    source_event_id UUID NOT NULL,
    source_contract_version INTEGER NOT NULL CHECK (source_contract_version > 0),
    source_event_version INTEGER NOT NULL CHECK (source_event_version > 0),
    source_actor_ref TEXT NOT NULL CHECK (btrim(source_actor_ref) <> '' AND char_length(source_actor_ref) <= 255),
    source_actor_kind TEXT NOT NULL CHECK (source_actor_kind IN ('platform_user', 'tenant_user', 'service', 'system_worker')),
    source_correlation_id UUID NOT NULL,
    source_causation_id UUID,
    source_idempotency_key TEXT NOT NULL CHECK (btrim(source_idempotency_key) <> '' AND char_length(source_idempotency_key) <= 512),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, entry_id)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.legal_projection_checkpoints (
    holding_ref TEXT NOT NULL,
    projection_name TEXT NOT NULL CHECK (projection_name = 'legal_documents'),
    aggregate_type TEXT NOT NULL CHECK (aggregate_type IN ('legal_contract', 'legal_document', 'legal_access_history')),
    aggregate_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    last_event_id UUID NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, projection_name, aggregate_type, aggregate_id)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.legal_projection_event_receipts (
    holding_ref TEXT NOT NULL,
    projection_name TEXT NOT NULL CHECK (projection_name = 'legal_documents'),
    event_id UUID NOT NULL,
    aggregate_type TEXT NOT NULL CHECK (aggregate_type IN ('legal_contract', 'legal_document', 'legal_access_history')),
    aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    event_type TEXT NOT NULL CHECK (event_type IN ('legal.contract_saved.v1', 'legal.document_saved.v1', 'legal.access_recorded.v1')),
    source_contract_version INTEGER NOT NULL CHECK (source_contract_version > 0),
    source_event_version INTEGER NOT NULL CHECK (source_event_version > 0),
    source_actor_ref TEXT NOT NULL CHECK (btrim(source_actor_ref) <> '' AND char_length(source_actor_ref) <= 255),
    source_actor_kind TEXT NOT NULL CHECK (source_actor_kind IN ('platform_user', 'tenant_user', 'service', 'system_worker')),
    source_correlation_id UUID NOT NULL,
    source_causation_id UUID,
    source_idempotency_key TEXT NOT NULL CHECK (btrim(source_idempotency_key) <> '' AND char_length(source_idempotency_key) <= 512),
    received_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    processed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, projection_name, event_id)
);

CREATE INDEX IF NOT EXISTS legal_contract_read_scope_idx
    ON erpx_tenant.legal_contract_read_rows
       (holding_ref, holding_id, company_id, branch_id, status, projected_at DESC);
CREATE INDEX IF NOT EXISTS legal_contract_party_read_scope_idx
    ON erpx_tenant.legal_contract_party_read_rows
       (holding_ref, holding_id, contract_id, party_id);
CREATE INDEX IF NOT EXISTS legal_contract_approval_read_scope_idx
    ON erpx_tenant.legal_contract_approval_read_rows
       (holding_ref, holding_id, contract_id, approved_at DESC);
CREATE INDEX IF NOT EXISTS legal_contract_retention_read_scope_idx
    ON erpx_tenant.legal_contract_retention_read_rows
       (holding_ref, holding_id, retention_until);
CREATE INDEX IF NOT EXISTS legal_document_read_scope_idx
    ON erpx_tenant.legal_document_read_rows
       (holding_ref, holding_id, company_id, branch_id, archived, projected_at DESC);
CREATE INDEX IF NOT EXISTS legal_document_version_read_scope_idx
    ON erpx_tenant.legal_document_version_read_rows
       (holding_ref, holding_id, document_id, version);
CREATE INDEX IF NOT EXISTS legal_access_history_read_scope_idx
    ON erpx_tenant.legal_access_history_read_rows
       (holding_ref, holding_id, company_id, branch_id, observed_at DESC, entry_id);
CREATE INDEX IF NOT EXISTS legal_projection_checkpoint_source_idx
    ON erpx_tenant.legal_projection_checkpoints
       (holding_ref, projection_name, aggregate_type, source_version, projected_at DESC);
CREATE INDEX IF NOT EXISTS legal_projection_receipt_source_idx
    ON erpx_tenant.legal_projection_event_receipts
       (holding_ref, projection_name, aggregate_type, aggregate_id, aggregate_version);

ALTER TABLE erpx_tenant.legal_contract_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.legal_contract_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.legal_contract_party_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.legal_contract_party_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.legal_contract_approval_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.legal_contract_approval_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.legal_contract_retention_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.legal_contract_retention_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.legal_document_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.legal_document_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.legal_document_version_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.legal_document_version_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.legal_access_history_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.legal_access_history_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.legal_projection_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.legal_projection_checkpoints FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.legal_projection_event_receipts ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.legal_projection_event_receipts FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_legal_contract_read_rows ON erpx_tenant.legal_contract_read_rows;
CREATE POLICY tenant_scope_legal_contract_read_rows ON erpx_tenant.legal_contract_read_rows
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_legal_contract_party_read_rows ON erpx_tenant.legal_contract_party_read_rows;
CREATE POLICY tenant_scope_legal_contract_party_read_rows ON erpx_tenant.legal_contract_party_read_rows
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_legal_contract_approval_read_rows ON erpx_tenant.legal_contract_approval_read_rows;
CREATE POLICY tenant_scope_legal_contract_approval_read_rows ON erpx_tenant.legal_contract_approval_read_rows
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_legal_contract_retention_read_rows ON erpx_tenant.legal_contract_retention_read_rows;
CREATE POLICY tenant_scope_legal_contract_retention_read_rows ON erpx_tenant.legal_contract_retention_read_rows
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_legal_document_read_rows ON erpx_tenant.legal_document_read_rows;
CREATE POLICY tenant_scope_legal_document_read_rows ON erpx_tenant.legal_document_read_rows
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_legal_document_version_read_rows ON erpx_tenant.legal_document_version_read_rows;
CREATE POLICY tenant_scope_legal_document_version_read_rows ON erpx_tenant.legal_document_version_read_rows
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_legal_access_history_read_rows ON erpx_tenant.legal_access_history_read_rows;
CREATE POLICY tenant_scope_legal_access_history_read_rows ON erpx_tenant.legal_access_history_read_rows
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_legal_projection_checkpoints ON erpx_tenant.legal_projection_checkpoints;
CREATE POLICY tenant_scope_legal_projection_checkpoints ON erpx_tenant.legal_projection_checkpoints
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_legal_projection_event_receipts ON erpx_tenant.legal_projection_event_receipts;
CREATE POLICY tenant_scope_legal_projection_event_receipts ON erpx_tenant.legal_projection_event_receipts
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
