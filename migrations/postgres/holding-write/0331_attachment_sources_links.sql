-- Canonical private file sources, purpose-scoped links and publication records.
-- Existing attachments remain readable as legacy_unscanned sources until the
-- explicit backfill/rescan command promotes them.
ALTER TABLE attachments
  ADD COLUMN company_id TEXT,
  ADD COLUMN detected_media_type TEXT,
  ADD COLUMN source_state TEXT NOT NULL DEFAULT 'legacy_unscanned'
    CHECK (source_state IN ('legacy_unscanned','quarantined','scanning','available','scan_error','malware_rejected','destroyed')),
  ADD COLUMN scanned_at BIGINT,
  ADD COLUMN scanner_definition_at BIGINT,
  ADD COLUMN preview_object_key TEXT UNIQUE,
  ADD COLUMN preview_media_type TEXT,
  ADD COLUMN preview_checksum_sha256 TEXT,
  ADD COLUMN preview_bytes BIGINT CHECK (preview_bytes IS NULL OR preview_bytes > 0),
  ADD COLUMN destroyed_at BIGINT,
  ADD COLUMN legacy_sanitized_only BOOLEAN NOT NULL DEFAULT false;

CREATE INDEX attachments_company_state ON attachments(company_id, source_state, created_at);
CREATE OR REPLACE FUNCTION attachment_source_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP='DELETE' THEN
    RAISE EXCEPTION 'ATTACHMENT_SOURCE_IMMUTABLE';
  END IF;
  IF NEW.source_state IS DISTINCT FROM OLD.source_state AND NOT (
    (OLD.source_state='legacy_unscanned' AND NEW.source_state='quarantined')
    OR (OLD.source_state='quarantined' AND NEW.source_state='scanning')
    OR (OLD.source_state='scanning' AND NEW.source_state IN ('available','scan_error','malware_rejected'))
    OR (OLD.source_state='scan_error' AND NEW.source_state IN ('scanning','destroyed'))
    OR (OLD.source_state IN ('available','malware_rejected') AND NEW.source_state='destroyed')
  ) THEN
    RAISE EXCEPTION 'ATTACHMENT_SOURCE_STATE_TRANSITION';
  END IF;
  IF OLD.legal_hold<>0 AND NEW.source_state='destroyed' THEN
    RAISE EXCEPTION 'ATTACHMENT_LEGAL_HOLD';
  END IF;
  IF NEW.object_key IS DISTINCT FROM OLD.object_key
    OR NEW.bytes IS DISTINCT FROM OLD.bytes
    OR NEW.checksum_sha256 IS DISTINCT FROM OLD.checksum_sha256
    OR NEW.original_name IS DISTINCT FROM OLD.original_name
    OR NEW.media_type IS DISTINCT FROM OLD.media_type
    OR (NEW.detected_media_type IS DISTINCT FROM OLD.detected_media_type AND OLD.detected_media_type IS NOT NULL)
    OR NEW.owner_type IS DISTINCT FROM OLD.owner_type
    OR NEW.owner_id IS DISTINCT FROM OLD.owner_id
    OR NEW.purpose IS DISTINCT FROM OLD.purpose
    OR NEW.revision IS DISTINCT FROM OLD.revision
    OR NEW.upload_id IS DISTINCT FROM OLD.upload_id
    OR NEW.created_by IS DISTINCT FROM OLD.created_by
    OR NEW.created_at IS DISTINCT FROM OLD.created_at
    OR (NEW.legacy_sanitized_only IS DISTINCT FROM OLD.legacy_sanitized_only
      AND NOT (OLD.legacy_sanitized_only=false AND NEW.legacy_sanitized_only=true AND OLD.company_id IS NULL AND OLD.owner_type='holding_brand'))
    OR (NEW.company_id IS DISTINCT FROM OLD.company_id AND NOT (OLD.company_id IS NULL AND NEW.company_id IS NOT NULL)) THEN
    RAISE EXCEPTION 'ATTACHMENT_SOURCE_IMMUTABLE';
  END IF;
  IF OLD.source_state IN ('available','malware_rejected','destroyed') AND (
    NEW.detected_media_type IS DISTINCT FROM OLD.detected_media_type
    OR NEW.scanned_at IS DISTINCT FROM OLD.scanned_at
    OR NEW.scanner_definition_at IS DISTINCT FROM OLD.scanner_definition_at
    OR NEW.preview_object_key IS DISTINCT FROM OLD.preview_object_key
    OR NEW.preview_media_type IS DISTINCT FROM OLD.preview_media_type
    OR NEW.preview_checksum_sha256 IS DISTINCT FROM OLD.preview_checksum_sha256
    OR NEW.preview_bytes IS DISTINCT FROM OLD.preview_bytes) THEN
    RAISE EXCEPTION 'ATTACHMENT_SOURCE_SCAN_IMMUTABLE';
  END IF;
  IF NEW.source_state='available' AND (NEW.detected_media_type IS NULL OR NEW.scanned_at IS NULL OR NEW.scanner_definition_at IS NULL) THEN
    RAISE EXCEPTION 'ATTACHMENT_SOURCE_SCAN_INCOMPLETE';
  END IF;
  IF (NEW.preview_object_key IS NULL AND (NEW.preview_media_type IS NOT NULL OR NEW.preview_checksum_sha256 IS NOT NULL OR NEW.preview_bytes IS NOT NULL))
    OR (NEW.preview_object_key IS NOT NULL AND (NEW.preview_media_type IS NULL OR NEW.preview_checksum_sha256 IS NULL OR NEW.preview_bytes IS NULL)) THEN
    RAISE EXCEPTION 'ATTACHMENT_PREVIEW_INCOMPLETE';
  END IF;
  IF NEW.source_state='destroyed' AND NEW.destroyed_at IS NULL THEN
    RAISE EXCEPTION 'ATTACHMENT_DESTRUCTION_INCOMPLETE';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER attachment_source_immutable BEFORE UPDATE OR DELETE ON attachments
  FOR EACH ROW EXECUTE FUNCTION attachment_source_immutable();

-- These two live aggregates predate universal file binding. Give attachment
-- optimistic concurrency a real source version and keep it current even when
-- their existing domain services update other columns directly.
ALTER TABLE inventory_lots ADD COLUMN version BIGINT NOT NULL DEFAULT 1 CHECK(version > 0);
ALTER TABLE marketing_campaigns ADD COLUMN version BIGINT NOT NULL DEFAULT 1 CHECK(version > 0);
CREATE OR REPLACE FUNCTION attachment_target_version() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW IS DISTINCT FROM OLD AND NEW.version=OLD.version THEN NEW.version:=OLD.version+1; END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER inventory_lot_attachment_version BEFORE UPDATE ON inventory_lots
  FOR EACH ROW EXECUTE FUNCTION attachment_target_version();
CREATE TRIGGER marketing_campaign_attachment_version BEFORE UPDATE ON marketing_campaigns
  FOR EACH ROW EXECUTE FUNCTION attachment_target_version();

CREATE TABLE attachment_sets (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  branch_id TEXT,
  department_id TEXT,
  target_type TEXT NOT NULL,
  target_id TEXT NOT NULL,
  component_kind TEXT NOT NULL
    CHECK (component_kind IN ('record','action','line','event','message','publication')),
  component_ref TEXT,
  purpose_key TEXT NOT NULL,
  expected_target_version BIGINT NOT NULL CHECK (expected_target_version > 0),
  policy_version BIGINT NOT NULL CHECK (policy_version > 0),
  classification TEXT NOT NULL
    CHECK (classification IN ('public','internal','confidential','restricted')),
  audience_json TEXT NOT NULL CHECK (erpx_json_valid(audience_json)),
  retention_policy_key TEXT,
  retention_policy_version BIGINT CHECK (retention_policy_version IS NULL OR retention_policy_version > 0),
  retain_until BIGINT,
  destruction_mode TEXT CHECK (destruction_mode IS NULL OR destruction_mode IN ('review','automatic')),
  required_count BIGINT NOT NULL DEFAULT 0 CHECK (required_count BETWEEN 0 AND 100),
  max_files BIGINT NOT NULL DEFAULT 10 CHECK (max_files BETWEEN 1 AND 100),
  state TEXT NOT NULL DEFAULT 'draft' CHECK (state IN ('draft','bound','withdrawn','expired')),
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL,
  bound_at BIGINT,
  bound_command_id TEXT,
  CHECK ((state='bound')=(bound_at IS NOT NULL AND bound_command_id IS NOT NULL))
);
CREATE INDEX attachment_sets_target ON attachment_sets(company_id,target_type,target_id,purpose_key,created_at DESC);
CREATE UNIQUE INDEX attachment_sets_active_identity ON attachment_sets(
  company_id,target_type,target_id,component_kind,COALESCE(component_ref,''),purpose_key,
  expected_target_version,created_by
) WHERE state='draft';

CREATE TABLE attachment_links (
  id TEXT PRIMARY KEY,
  set_id TEXT NOT NULL REFERENCES attachment_sets(id),
  attachment_id TEXT REFERENCES attachments(id),
  upload_intent_id TEXT,
  caption TEXT,
  state TEXT NOT NULL DEFAULT 'draft' CHECK (state IN ('draft','bound','withdrawn','expired')),
  supersedes_link_id TEXT REFERENCES attachment_links(id),
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  expires_at BIGINT NOT NULL,
  bound_at BIGINT,
  binding_type TEXT,
  binding_id TEXT,
  withdrawn_at BIGINT,
  withdrawn_by TEXT,
  withdrawal_reason TEXT,
  CHECK ((state='bound')=(bound_at IS NOT NULL AND binding_type IS NOT NULL AND binding_id IS NOT NULL)),
  CHECK ((state='withdrawn')=(withdrawn_at IS NOT NULL AND withdrawn_by IS NOT NULL AND withdrawal_reason IS NOT NULL))
);
CREATE INDEX attachment_links_set ON attachment_links(set_id,state,created_at);
CREATE UNIQUE INDEX attachment_links_source_set ON attachment_links(set_id,attachment_id) WHERE attachment_id IS NOT NULL AND state IN ('draft','bound');

ALTER TABLE upload_intents
  ADD COLUMN attachment_set_id TEXT REFERENCES attachment_sets(id),
  ADD COLUMN attachment_link_id TEXT REFERENCES attachment_links(id),
  ADD COLUMN company_id TEXT,
  ADD COLUMN detected_media_type TEXT,
  ADD COLUMN quarantine_key TEXT;
ALTER TABLE attachment_links
  ADD CONSTRAINT attachment_links_upload_intent_fk FOREIGN KEY(upload_intent_id) REFERENCES upload_intents(id);
CREATE UNIQUE INDEX upload_intents_attachment_link ON upload_intents(attachment_link_id) WHERE attachment_link_id IS NOT NULL;

CREATE TABLE attachment_scan_attempts (
  id TEXT PRIMARY KEY,
  attachment_id TEXT NOT NULL REFERENCES attachments(id),
  attempt BIGINT NOT NULL CHECK (attempt BETWEEN 1 AND 12),
  state TEXT NOT NULL DEFAULT 'pending' CHECK (state IN ('pending','running','clean','infected','error')),
  scanner_engine TEXT NOT NULL DEFAULT 'clamd',
  scanner_signature TEXT,
  scanner_definition_at BIGINT,
  claimed_at BIGINT,
  claim_until BIGINT,
  lease_token TEXT,
  result_code TEXT,
  error_code TEXT,
  created_at BIGINT NOT NULL,
  completed_at BIGINT,
  UNIQUE(attachment_id,attempt)
);
CREATE INDEX attachment_scan_pending ON attachment_scan_attempts(state,claim_until,created_at);

CREATE TABLE attachment_events (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  attachment_id TEXT REFERENCES attachments(id),
  link_id TEXT REFERENCES attachment_links(id),
  set_id TEXT REFERENCES attachment_sets(id),
  actor_id TEXT NOT NULL,
  action TEXT NOT NULL,
  details_json TEXT NOT NULL CHECK (erpx_json_valid(details_json)),
  created_at BIGINT NOT NULL
);
CREATE INDEX attachment_events_source ON attachment_events(attachment_id,created_at);
CREATE INDEX attachment_events_target ON attachment_events(set_id,created_at);

CREATE TABLE attachment_access_events (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  attachment_id TEXT NOT NULL REFERENCES attachments(id),
  link_id TEXT NOT NULL REFERENCES attachment_links(id),
  actor_id TEXT NOT NULL,
  variant TEXT NOT NULL CHECK (variant IN ('original','preview','status')),
  outcome TEXT NOT NULL CHECK (outcome IN ('allowed','denied')),
  reason TEXT,
  created_at BIGINT NOT NULL
);
CREATE INDEX attachment_access_source ON attachment_access_events(attachment_id,created_at);

CREATE TABLE attachment_publications (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  source_link_id TEXT NOT NULL REFERENCES attachment_links(id),
  target_type TEXT NOT NULL,
  target_id TEXT NOT NULL,
  derivative_object_key TEXT NOT NULL UNIQUE,
  derivative_media_type TEXT NOT NULL CHECK (derivative_media_type IN ('image/jpeg','image/webp')),
  derivative_checksum_sha256 TEXT NOT NULL,
  derivative_bytes BIGINT NOT NULL CHECK (derivative_bytes > 0),
  alt_text TEXT NOT NULL CHECK (length(alt_text) BETWEEN 1 AND 500),
  ordinal BIGINT NOT NULL DEFAULT 0,
  revision BIGINT NOT NULL CHECK (revision > 0),
  state TEXT NOT NULL DEFAULT 'draft' CHECK (state IN ('draft','approved','revoked')),
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  approved_by TEXT,
  approved_at BIGINT,
  revoked_by TEXT,
  revoked_at BIGINT,
  revocation_reason TEXT,
  CHECK (state='draft' OR (approved_by IS NOT NULL AND approved_at IS NOT NULL)),
  CHECK (state<>'revoked' OR (revoked_by IS NOT NULL AND revoked_at IS NOT NULL AND revocation_reason IS NOT NULL))
);
CREATE INDEX attachment_publications_target ON attachment_publications(company_id,target_type,target_id,state,ordinal,revision DESC);
CREATE UNIQUE INDEX attachment_publications_revision ON attachment_publications(company_id,target_type,target_id,revision);

CREATE OR REPLACE FUNCTION attachment_publication_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP='DELETE' THEN RAISE EXCEPTION 'ATTACHMENT_PUBLICATION_IMMUTABLE'; END IF;
  IF NEW.id IS DISTINCT FROM OLD.id
    OR NEW.company_id IS DISTINCT FROM OLD.company_id
    OR NEW.source_link_id IS DISTINCT FROM OLD.source_link_id
    OR NEW.target_type IS DISTINCT FROM OLD.target_type
    OR NEW.target_id IS DISTINCT FROM OLD.target_id
    OR NEW.derivative_object_key IS DISTINCT FROM OLD.derivative_object_key
    OR NEW.derivative_media_type IS DISTINCT FROM OLD.derivative_media_type
    OR NEW.derivative_checksum_sha256 IS DISTINCT FROM OLD.derivative_checksum_sha256
    OR NEW.derivative_bytes IS DISTINCT FROM OLD.derivative_bytes
    OR NEW.alt_text IS DISTINCT FROM OLD.alt_text
    OR NEW.revision IS DISTINCT FROM OLD.revision
    OR NEW.created_by IS DISTINCT FROM OLD.created_by
    OR NEW.created_at IS DISTINCT FROM OLD.created_at THEN
    RAISE EXCEPTION 'ATTACHMENT_PUBLICATION_IMMUTABLE';
  END IF;
  IF NEW.state IS DISTINCT FROM OLD.state AND NOT (
    (OLD.state='draft' AND NEW.state='approved')
    OR (OLD.state='approved' AND NEW.state='revoked')
  ) THEN
    RAISE EXCEPTION 'ATTACHMENT_PUBLICATION_STATE_TRANSITION';
  END IF;
  IF OLD.state='draft' AND NEW.state='draft' AND (
    NEW.approved_by IS DISTINCT FROM OLD.approved_by
    OR NEW.approved_at IS DISTINCT FROM OLD.approved_at
    OR NEW.revoked_by IS DISTINCT FROM OLD.revoked_by
    OR NEW.revoked_at IS DISTINCT FROM OLD.revoked_at
    OR NEW.revocation_reason IS DISTINCT FROM OLD.revocation_reason) THEN
    RAISE EXCEPTION 'ATTACHMENT_PUBLICATION_REVIEW_IMMUTABLE';
  END IF;
  IF OLD.state='approved' AND NEW.state='approved' AND (
    NEW.approved_by IS DISTINCT FROM OLD.approved_by
    OR NEW.approved_at IS DISTINCT FROM OLD.approved_at
    OR NEW.revoked_by IS DISTINCT FROM OLD.revoked_by
    OR NEW.revoked_at IS DISTINCT FROM OLD.revoked_at
    OR NEW.revocation_reason IS DISTINCT FROM OLD.revocation_reason) THEN
    RAISE EXCEPTION 'ATTACHMENT_PUBLICATION_REVIEW_IMMUTABLE';
  END IF;
  IF OLD.state='revoked' AND NEW IS DISTINCT FROM OLD THEN
    RAISE EXCEPTION 'ATTACHMENT_PUBLICATION_IMMUTABLE';
  END IF;
  IF NEW.state='draft' AND (NEW.approved_by IS NOT NULL OR NEW.approved_at IS NOT NULL OR NEW.revoked_by IS NOT NULL OR NEW.revoked_at IS NOT NULL OR NEW.revocation_reason IS NOT NULL) THEN
    RAISE EXCEPTION 'ATTACHMENT_PUBLICATION_REVIEW_INCOMPLETE';
  END IF;
  IF NEW.state='approved' AND (NEW.approved_by IS NULL OR NEW.approved_at IS NULL OR NEW.approved_by=NEW.created_by OR NEW.revoked_by IS NOT NULL OR NEW.revoked_at IS NOT NULL OR NEW.revocation_reason IS NOT NULL) THEN
    RAISE EXCEPTION 'ATTACHMENT_PUBLICATION_REVIEW_INCOMPLETE';
  END IF;
  IF NEW.state='revoked' AND (NEW.approved_by IS NULL OR NEW.approved_at IS NULL OR NEW.revoked_by IS NULL OR NEW.revoked_at IS NULL OR NEW.revocation_reason IS NULL) THEN
    RAISE EXCEPTION 'ATTACHMENT_PUBLICATION_REVOCATION_INCOMPLETE';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER attachment_publication_immutable BEFORE UPDATE OR DELETE ON attachment_publications
  FOR EACH ROW EXECUTE FUNCTION attachment_publication_immutable();

CREATE TABLE attachment_publication_orders (
  company_id TEXT NOT NULL,
  target_type TEXT NOT NULL,
  target_id TEXT NOT NULL,
  version BIGINT NOT NULL DEFAULT 1 CHECK(version>0),
  updated_by TEXT NOT NULL,
  updated_at BIGINT NOT NULL,
  PRIMARY KEY(company_id,target_type,target_id)
);

CREATE TABLE attachment_storage_ledger (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  attachment_id TEXT NOT NULL REFERENCES attachments(id),
  variant TEXT NOT NULL CHECK (variant IN ('original','preview','publication')),
  object_key TEXT NOT NULL UNIQUE,
  bytes BIGINT NOT NULL CHECK (bytes >= 0),
  state TEXT NOT NULL CHECK (state IN ('reserved','stored','destroyed')),
  created_at BIGINT NOT NULL,
  destroyed_at BIGINT,
  UNIQUE(attachment_id,variant,object_key)
);
CREATE INDEX attachment_storage_company ON attachment_storage_ledger(company_id,state);

CREATE OR REPLACE FUNCTION attachment_storage_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP='DELETE' THEN RAISE EXCEPTION 'ATTACHMENT_STORAGE_IMMUTABLE'; END IF;
  IF NEW.id IS DISTINCT FROM OLD.id
    OR NEW.company_id IS DISTINCT FROM OLD.company_id
    OR NEW.attachment_id IS DISTINCT FROM OLD.attachment_id
    OR NEW.variant IS DISTINCT FROM OLD.variant
    OR NEW.object_key IS DISTINCT FROM OLD.object_key
    OR NEW.bytes IS DISTINCT FROM OLD.bytes
    OR NEW.created_at IS DISTINCT FROM OLD.created_at THEN
    RAISE EXCEPTION 'ATTACHMENT_STORAGE_IMMUTABLE';
  END IF;
  IF NEW.state IS DISTINCT FROM OLD.state AND NOT (
    (OLD.state='reserved' AND NEW.state IN ('stored','destroyed'))
    OR (OLD.state='stored' AND NEW.state='destroyed')
  ) THEN
    RAISE EXCEPTION 'ATTACHMENT_STORAGE_STATE_TRANSITION';
  END IF;
  IF OLD.state='destroyed' AND NEW IS DISTINCT FROM OLD THEN
    RAISE EXCEPTION 'ATTACHMENT_STORAGE_IMMUTABLE';
  END IF;
  IF NEW.state='destroyed' AND NEW.destroyed_at IS NULL THEN
    RAISE EXCEPTION 'ATTACHMENT_STORAGE_DESTRUCTION_INCOMPLETE';
  END IF;
  IF NEW.state<>'destroyed' AND NEW.destroyed_at IS NOT NULL THEN
    RAISE EXCEPTION 'ATTACHMENT_STORAGE_DESTRUCTION_INCOMPLETE';
  END IF;
  IF NEW.state='destroyed' AND EXISTS(SELECT 1 FROM attachments WHERE id=NEW.attachment_id AND legal_hold<>0) THEN
    RAISE EXCEPTION 'ATTACHMENT_LEGAL_HOLD';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER attachment_storage_immutable BEFORE UPDATE OR DELETE ON attachment_storage_ledger
  FOR EACH ROW EXECUTE FUNCTION attachment_storage_immutable();

CREATE TABLE attachment_reconciliation_runs (
  id TEXT PRIMARY KEY,
  mode TEXT NOT NULL CHECK(mode IN ('dry_run','final')),
  upload_frozen BOOLEAN NOT NULL,
  checksums_verified BOOLEAN NOT NULL,
  scanner_engine TEXT,
  scanner_definition_at BIGINT,
  ready BOOLEAN NOT NULL,
  report_json TEXT NOT NULL CHECK(erpx_json_valid(report_json)),
  created_at BIGINT NOT NULL
);
CREATE INDEX attachment_reconciliation_latest ON attachment_reconciliation_runs(created_at DESC);

CREATE OR REPLACE FUNCTION attachment_link_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF OLD.state='bound' AND NEW IS DISTINCT FROM OLD THEN
    RAISE EXCEPTION 'BOUND_ATTACHMENT_LINK_IMMUTABLE';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER attachment_link_immutable BEFORE UPDATE OR DELETE ON attachment_links
  FOR EACH ROW EXECUTE FUNCTION attachment_link_immutable();

CREATE OR REPLACE FUNCTION attachment_bound_set_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF OLD.state='bound' AND (TG_OP='DELETE'
    OR NEW.company_id IS DISTINCT FROM OLD.company_id
    OR NEW.branch_id IS DISTINCT FROM OLD.branch_id
    OR NEW.department_id IS DISTINCT FROM OLD.department_id
    OR NEW.target_type IS DISTINCT FROM OLD.target_type
    OR NEW.target_id IS DISTINCT FROM OLD.target_id
    OR NEW.component_kind IS DISTINCT FROM OLD.component_kind
    OR NEW.component_ref IS DISTINCT FROM OLD.component_ref
    OR NEW.purpose_key IS DISTINCT FROM OLD.purpose_key
    OR NEW.expected_target_version IS DISTINCT FROM OLD.expected_target_version
    OR NEW.policy_version IS DISTINCT FROM OLD.policy_version
    OR NEW.classification IS DISTINCT FROM OLD.classification
    OR NEW.audience_json IS DISTINCT FROM OLD.audience_json
    OR NEW.retention_policy_key IS DISTINCT FROM OLD.retention_policy_key
    OR NEW.retention_policy_version IS DISTINCT FROM OLD.retention_policy_version
    OR NEW.retain_until IS DISTINCT FROM OLD.retain_until
    OR NEW.destruction_mode IS DISTINCT FROM OLD.destruction_mode
    OR NEW.required_count IS DISTINCT FROM OLD.required_count
    OR NEW.max_files IS DISTINCT FROM OLD.max_files
    OR NEW.state IS DISTINCT FROM OLD.state
    OR NEW.created_by IS DISTINCT FROM OLD.created_by
    OR NEW.created_at IS DISTINCT FROM OLD.created_at
    OR NEW.updated_at IS DISTINCT FROM OLD.updated_at
    OR NEW.bound_at IS DISTINCT FROM OLD.bound_at
    OR NEW.bound_command_id IS DISTINCT FROM OLD.bound_command_id) THEN
    RAISE EXCEPTION 'BOUND_ATTACHMENT_SET_IMMUTABLE';
  END IF;
  IF TG_OP='DELETE' THEN RETURN OLD; END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER attachment_bound_set_immutable BEFORE UPDATE OR DELETE ON attachment_sets
  FOR EACH ROW EXECUTE FUNCTION attachment_bound_set_immutable();

CREATE OR REPLACE FUNCTION attachment_append_only() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION 'ATTACHMENT_AUDIT_APPEND_ONLY';
END $$;
CREATE TRIGGER attachment_events_append_only BEFORE UPDATE OR DELETE ON attachment_events
  FOR EACH ROW EXECUTE FUNCTION attachment_append_only();
CREATE TRIGGER attachment_access_events_append_only BEFORE UPDATE OR DELETE ON attachment_access_events
  FOR EACH ROW EXECUTE FUNCTION attachment_append_only();
CREATE TRIGGER attachment_reconciliation_append_only BEFORE UPDATE OR DELETE ON attachment_reconciliation_runs
  FOR EACH ROW EXECUTE FUNCTION attachment_append_only();

DO $$
DECLARE migration_role TEXT := current_user; app_role TEXT; jobs_role TEXT;
BEGIN
  IF migration_role !~ '_migration$' THEN RETURN; END IF;
  app_role := regexp_replace(migration_role, '_migration$', '_app');
  jobs_role := regexp_replace(migration_role, '_migration$', '_jobs');
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname=app_role)
    OR NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname=jobs_role) THEN
    RAISE EXCEPTION 'ERPX_RUNTIME_ROLES_MISSING';
  END IF;
  EXECUTE format('GRANT SELECT,INSERT,UPDATE ON TABLE attachment_sets,attachment_links,attachment_events,attachment_access_events,attachment_publications,attachment_publication_orders,attachment_storage_ledger TO %I',app_role);
  EXECUTE format('GRANT SELECT,INSERT ON TABLE attachment_reconciliation_runs TO %I,%I',app_role,jobs_role);
  EXECUTE format('GRANT SELECT,INSERT,UPDATE ON TABLE attachment_scan_attempts TO %I,%I',app_role,jobs_role);
  EXECUTE format('GRANT SELECT,UPDATE ON TABLE attachment_sets,attachment_links,attachments,upload_intents,attachment_storage_ledger TO %I',jobs_role);
  EXECUTE format('GRANT INSERT ON TABLE attachment_events TO %I',jobs_role);
END $$;
