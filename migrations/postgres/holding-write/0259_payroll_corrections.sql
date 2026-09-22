CREATE TABLE hr_payroll_corrections (
  id TEXT PRIMARY KEY,
  run_id TEXT NOT NULL REFERENCES hr_payroll_runs(id),
  company_id TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','approved','rejected','cancelled')),
  version BIGINT NOT NULL DEFAULT 1 CHECK(version > 0),
  run_version BIGINT NOT NULL,
  run_revision BIGINT NOT NULL,
  input_digest TEXT NOT NULL,
  totals_json TEXT NOT NULL CHECK(erpx_json_valid(totals_json)),
  reason TEXT NOT NULL,
  requested_by TEXT NOT NULL,
  reviewed_by TEXT,
  review_reason TEXT,
  created_at BIGINT NOT NULL,
  reviewed_at BIGINT,
  updated_at BIGINT NOT NULL
);
CREATE UNIQUE INDEX hr_payroll_correction_pending
  ON hr_payroll_corrections(run_id) WHERE status='pending';
CREATE INDEX hr_payroll_correction_queue
  ON hr_payroll_corrections(company_id,status,created_at DESC);

CREATE TABLE hr_payroll_correction_events (
  id TEXT PRIMARY KEY,
  correction_id TEXT NOT NULL REFERENCES hr_payroll_corrections(id),
  run_id TEXT NOT NULL REFERENCES hr_payroll_runs(id),
  company_id TEXT NOT NULL,
  action TEXT NOT NULL CHECK(action IN ('requested','approved','rejected','cancelled')),
  actor_id TEXT NOT NULL,
  run_version BIGINT NOT NULL,
  evidence_json TEXT NOT NULL CHECK(erpx_json_valid(evidence_json)),
  created_at BIGINT NOT NULL
);
CREATE INDEX hr_payroll_correction_history
  ON hr_payroll_correction_events(run_id,created_at,id);
CREATE TRIGGER hr_payroll_correction_event_immutable
  BEFORE UPDATE OR DELETE ON hr_payroll_correction_events
  FOR EACH ROW EXECUTE FUNCTION iam_immutable();
