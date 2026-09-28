ALTER TABLE billing_evidence
  ADD COLUMN object_key TEXT,
  ADD COLUMN detected_mime TEXT,
  ADD COLUMN source_state TEXT NOT NULL DEFAULT 'legacy_unscanned'
    CHECK (source_state IN ('legacy_unscanned','quarantined','scanning','available','scan_error','malware_rejected','destroyed')),
  ADD COLUMN scanned_at BIGINT,
  ADD COLUMN scanner_definition_at BIGINT,
  ADD COLUMN destroyed_at BIGINT;

-- Legacy billing evidence was stored at exactly this key. Historical bytes are
-- scanned in place; migrations never claim they are clean.
UPDATE billing_evidence SET object_key=holding_id||'/'||id WHERE object_key IS NULL;
ALTER TABLE billing_evidence ALTER COLUMN object_key SET NOT NULL;
CREATE UNIQUE INDEX billing_evidence_object_key ON billing_evidence(object_key);
CREATE INDEX billing_evidence_scan_state ON billing_evidence(source_state,created_at);

CREATE OR REPLACE FUNCTION billing_evidence_source_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP='DELETE' THEN
    RAISE EXCEPTION 'BILLING_EVIDENCE_SOURCE_IMMUTABLE';
  END IF;
  IF NEW.source_state IS DISTINCT FROM OLD.source_state AND NOT (
    (OLD.source_state='legacy_unscanned' AND NEW.source_state IN ('quarantined','scanning'))
    OR (OLD.source_state='quarantined' AND NEW.source_state='scanning')
    OR (OLD.source_state='scanning' AND NEW.source_state IN ('available','scan_error','malware_rejected'))
    OR (OLD.source_state='scan_error' AND NEW.source_state IN ('scanning','destroyed'))
    OR (OLD.source_state IN ('available','malware_rejected') AND NEW.source_state='destroyed')
  ) THEN
    RAISE EXCEPTION 'BILLING_EVIDENCE_STATE_TRANSITION';
  END IF;
  IF NEW.id IS DISTINCT FROM OLD.id
    OR NEW.holding_id IS DISTINCT FROM OLD.holding_id
    OR NEW.filename IS DISTINCT FROM OLD.filename
    OR NEW.mime IS DISTINCT FROM OLD.mime
    OR NEW.size IS DISTINCT FROM OLD.size
    OR NEW.sha256 IS DISTINCT FROM OLD.sha256
    OR NEW.actor IS DISTINCT FROM OLD.actor
    OR NEW.created_at IS DISTINCT FROM OLD.created_at
    OR NEW.object_key IS DISTINCT FROM OLD.object_key
    OR (NEW.detected_mime IS DISTINCT FROM OLD.detected_mime AND OLD.detected_mime IS NOT NULL) THEN
    RAISE EXCEPTION 'BILLING_EVIDENCE_SOURCE_IMMUTABLE';
  END IF;
  IF OLD.source_state IN ('available','malware_rejected','destroyed') AND (
    NEW.detected_mime IS DISTINCT FROM OLD.detected_mime
    OR NEW.scanned_at IS DISTINCT FROM OLD.scanned_at
    OR NEW.scanner_definition_at IS DISTINCT FROM OLD.scanner_definition_at) THEN
    RAISE EXCEPTION 'BILLING_EVIDENCE_SCAN_IMMUTABLE';
  END IF;
  IF NEW.source_state='available' AND (NEW.detected_mime IS NULL OR NEW.scanned_at IS NULL OR NEW.scanner_definition_at IS NULL) THEN
    RAISE EXCEPTION 'BILLING_EVIDENCE_SCAN_INCOMPLETE';
  END IF;
  IF NEW.source_state='destroyed' AND NEW.destroyed_at IS NULL THEN
    RAISE EXCEPTION 'BILLING_EVIDENCE_DESTRUCTION_INCOMPLETE';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER billing_evidence_source_immutable BEFORE UPDATE OR DELETE ON billing_evidence
  FOR EACH ROW EXECUTE FUNCTION billing_evidence_source_immutable();

CREATE TABLE billing_evidence_scan_attempts (
  id TEXT PRIMARY KEY,
  evidence_id TEXT NOT NULL REFERENCES billing_evidence(id),
  attempt BIGINT NOT NULL CHECK (attempt BETWEEN 1 AND 12),
  state TEXT NOT NULL DEFAULT 'pending' CHECK (state IN ('pending','running','clean','infected','error')),
  scanner_engine TEXT,
  scanner_signature TEXT,
  scanner_definition_at BIGINT,
  claimed_at BIGINT,
  claim_until BIGINT,
  lease_token TEXT,
  error_code TEXT,
  created_at BIGINT NOT NULL,
  completed_at BIGINT,
  UNIQUE(evidence_id,attempt)
);
CREATE INDEX billing_evidence_scan_pending ON billing_evidence_scan_attempts(state,claim_until,created_at);

INSERT INTO billing_evidence_scan_attempts(id,evidence_id,attempt,state,created_at)
SELECT 'billing-scan-'||id,id,1,'pending',created_at FROM billing_evidence
ON CONFLICT(evidence_id,attempt) DO NOTHING;

CREATE TABLE billing_evidence_access_events (
  id TEXT PRIMARY KEY,
  evidence_id TEXT NOT NULL REFERENCES billing_evidence(id),
  holding_id TEXT NOT NULL,
  actor TEXT NOT NULL,
  outcome TEXT NOT NULL CHECK (outcome IN ('allowed','denied')),
  reason TEXT,
  created_at BIGINT NOT NULL
);
CREATE INDEX billing_evidence_access_source ON billing_evidence_access_events(evidence_id,created_at);

CREATE TABLE billing_evidence_events (
  id TEXT PRIMARY KEY,
  evidence_id TEXT NOT NULL REFERENCES billing_evidence(id),
  holding_id TEXT NOT NULL,
  actor TEXT NOT NULL,
  action TEXT NOT NULL,
  details_json TEXT NOT NULL DEFAULT '{}' CHECK(erpx_json_valid(details_json)),
  created_at BIGINT NOT NULL
);
CREATE INDEX billing_evidence_events_source ON billing_evidence_events(evidence_id,created_at);

CREATE OR REPLACE FUNCTION billing_evidence_audit_append_only() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION 'BILLING_EVIDENCE_AUDIT_APPEND_ONLY';
END $$;
CREATE TRIGGER billing_evidence_access_append_only BEFORE UPDATE OR DELETE ON billing_evidence_access_events
  FOR EACH ROW EXECUTE FUNCTION billing_evidence_audit_append_only();
CREATE TRIGGER billing_evidence_events_append_only BEFORE UPDATE OR DELETE ON billing_evidence_events
  FOR EACH ROW EXECUTE FUNCTION billing_evidence_audit_append_only();
