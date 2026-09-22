CREATE TABLE hr_payroll_advances (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  engagement_id TEXT NOT NULL REFERENCES hr_engagements(id),
  person_id TEXT NOT NULL REFERENCES hr_people(id),
  status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','approved','rejected','cancelled','closed')),
  version BIGINT NOT NULL DEFAULT 1 CHECK(version > 0),
  amount_minor BIGINT NOT NULL CHECK(amount_minor > 0),
  installment_count INTEGER NOT NULL CHECK(installment_count BETWEEN 1 AND 24),
  first_deduction_period TEXT NOT NULL,
  currency TEXT NOT NULL CHECK(length(currency)=3 AND currency=upper(currency)),
  reason TEXT NOT NULL,
  requested_by TEXT NOT NULL,
  approved_by TEXT,
  review_reason TEXT,
  created_at BIGINT NOT NULL,
  reviewed_at BIGINT,
  updated_at BIGINT NOT NULL
);
CREATE INDEX hr_payroll_advances_queue ON hr_payroll_advances(company_id,status,created_at DESC);
CREATE INDEX hr_payroll_advances_person ON hr_payroll_advances(company_id,person_id,status);

CREATE TABLE hr_payroll_advance_repayments (
  id TEXT PRIMARY KEY,
  advance_id TEXT NOT NULL REFERENCES hr_payroll_advances(id),
  company_id TEXT NOT NULL,
  sequence INTEGER NOT NULL CHECK(sequence > 0),
  due_period_start TEXT NOT NULL,
  due_period_end TEXT NOT NULL,
  amount_minor BIGINT NOT NULL CHECK(amount_minor > 0),
  status TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open','reserved','collected','waived')),
  payroll_run_id TEXT REFERENCES hr_payroll_runs(id),
  version BIGINT NOT NULL DEFAULT 1 CHECK(version > 0),
  created_at BIGINT NOT NULL,
  UNIQUE(advance_id,sequence),
  CHECK(due_period_end >= due_period_start)
);
CREATE INDEX hr_payroll_advance_repayment_due ON hr_payroll_advance_repayments(company_id,status,due_period_start);

CREATE TABLE hr_payroll_advance_events (
  id TEXT PRIMARY KEY,
  advance_id TEXT NOT NULL REFERENCES hr_payroll_advances(id),
  company_id TEXT NOT NULL,
  action TEXT NOT NULL CHECK(action IN ('requested','approved','rejected','cancelled')),
  actor_id TEXT NOT NULL,
  advance_version BIGINT NOT NULL,
  evidence_json TEXT NOT NULL CHECK(erpx_json_valid(evidence_json)),
  created_at BIGINT NOT NULL
);
CREATE INDEX hr_payroll_advance_history ON hr_payroll_advance_events(advance_id,created_at,id);
CREATE TRIGGER hr_payroll_advance_event_immutable
  BEFORE UPDATE OR DELETE ON hr_payroll_advance_events
  FOR EACH ROW EXECUTE FUNCTION iam_immutable();
