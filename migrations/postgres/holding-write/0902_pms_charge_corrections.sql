CREATE TABLE pms_charge_corrections (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  property_id TEXT NOT NULL REFERENCES pms_properties(id),
  entry_id TEXT NOT NULL REFERENCES pms_records(id),
  folio_id TEXT NOT NULL REFERENCES pms_records(id),
  status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending','approved','rejected')),
  version BIGINT NOT NULL DEFAULT 1 CHECK (version > 0),
  reason TEXT NOT NULL,
  requested_by TEXT NOT NULL,
  reviewed_by TEXT,
  review_reason TEXT,
  reversal_entry_id TEXT REFERENCES pms_records(id),
  created_at BIGINT NOT NULL,
  reviewed_at BIGINT,
  updated_at BIGINT NOT NULL
);
CREATE UNIQUE INDEX pms_charge_correction_pending_entry
  ON pms_charge_corrections(entry_id) WHERE status='pending';
CREATE INDEX pms_charge_correction_queue
  ON pms_charge_corrections(company_id,property_id,status,created_at DESC);

CREATE TABLE pms_charge_correction_events (
  id TEXT PRIMARY KEY,
  correction_id TEXT NOT NULL REFERENCES pms_charge_corrections(id),
  company_id TEXT NOT NULL,
  property_id TEXT NOT NULL,
  entry_id TEXT NOT NULL,
  action TEXT NOT NULL CHECK (action IN ('requested','approved','rejected')),
  actor_id TEXT NOT NULL,
  correction_version BIGINT NOT NULL,
  reason TEXT NOT NULL,
  reversal_entry_id TEXT,
  created_at BIGINT NOT NULL
);
CREATE INDEX pms_charge_correction_history
  ON pms_charge_correction_events(correction_id,created_at,id);
CREATE TRIGGER pms_charge_correction_event_immutable
  BEFORE UPDATE OR DELETE ON pms_charge_correction_events
  FOR EACH ROW EXECUTE FUNCTION iam_immutable();
