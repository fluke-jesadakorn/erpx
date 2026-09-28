-- Billing evidence lives in the control database and a separate private object
-- prefix. Keep its physical-byte accounting and final reconciliation receipt
-- alongside that owner instead of leaking it into a Holding database.
CREATE TABLE billing_evidence_storage_ledger (
  id TEXT PRIMARY KEY,
  evidence_id TEXT NOT NULL REFERENCES billing_evidence(id),
  holding_id TEXT NOT NULL REFERENCES billing_accounts(holding_id),
  object_key TEXT NOT NULL UNIQUE,
  bytes BIGINT NOT NULL CHECK (bytes >= 0),
  state TEXT NOT NULL DEFAULT 'stored' CHECK (state IN ('stored','destroyed')),
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL
);
CREATE INDEX billing_evidence_storage_holding
  ON billing_evidence_storage_ledger(holding_id,state,created_at);

INSERT INTO billing_evidence_storage_ledger(
  id,evidence_id,holding_id,object_key,bytes,state,created_at,updated_at
)
SELECT 'billing-storage-'||id,id,holding_id,object_key,size,
  CASE WHEN source_state='destroyed' THEN 'destroyed' ELSE 'stored' END,
  created_at,COALESCE(destroyed_at,created_at)
FROM billing_evidence
ON CONFLICT(object_key) DO NOTHING;

CREATE TABLE billing_evidence_reconciliation_runs (
  id TEXT PRIMARY KEY,
  mode TEXT NOT NULL CHECK (mode IN ('audit','final')),
  upload_frozen BOOLEAN NOT NULL,
  checksums_verified BOOLEAN NOT NULL,
  scanner_engine TEXT,
  scanner_definition_at BIGINT,
  ready BOOLEAN NOT NULL,
  report_json TEXT NOT NULL CHECK(erpx_json_valid(report_json)),
  created_at BIGINT NOT NULL
);
CREATE INDEX billing_evidence_reconciliation_latest
  ON billing_evidence_reconciliation_runs(created_at DESC);
CREATE TRIGGER billing_evidence_reconciliation_append_only
  BEFORE UPDATE OR DELETE ON billing_evidence_reconciliation_runs
  FOR EACH ROW EXECUTE FUNCTION billing_evidence_audit_append_only();

-- Rejected or indeterminate bytes remain sealed until an explicit approved
-- security-destruction policy exists. Ordinary retention must not destroy them.
CREATE OR REPLACE FUNCTION billing_evidence_security_seal() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF OLD.source_state IN ('scan_error','malware_rejected') AND NEW.source_state='destroyed' THEN
    RAISE EXCEPTION 'BILLING_EVIDENCE_SECURITY_DESTRUCTION_POLICY_REQUIRED';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER billing_evidence_security_seal
  BEFORE UPDATE ON billing_evidence
  FOR EACH ROW EXECUTE FUNCTION billing_evidence_security_seal();
