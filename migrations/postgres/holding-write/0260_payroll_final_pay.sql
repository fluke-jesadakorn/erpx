ALTER TABLE hr_payroll_runs ADD COLUMN scope_engagement_id TEXT REFERENCES hr_engagements(id);
ALTER TABLE hr_payroll_runs DROP CONSTRAINT hr_payroll_runs_mode_check;
ALTER TABLE hr_payroll_runs ADD CONSTRAINT hr_payroll_runs_mode_check CHECK(mode IN ('preview','final_pay','production'));
CREATE INDEX hr_payroll_runs_final_pay_scope ON hr_payroll_runs(company_id,scope_engagement_id) WHERE mode='final_pay';

CREATE TABLE hr_payroll_final_pay_preparations (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  task_id TEXT NOT NULL UNIQUE REFERENCES people_preparation_tasks(id),
  case_id TEXT NOT NULL REFERENCES people_lifecycle_cases(id),
  engagement_id TEXT NOT NULL REFERENCES hr_engagements(id),
  run_id TEXT NOT NULL UNIQUE REFERENCES hr_payroll_runs(id),
  status TEXT NOT NULL DEFAULT 'prepared' CHECK(status IN ('prepared','cancelled')),
  version BIGINT NOT NULL DEFAULT 1 CHECK(version > 0),
  reason TEXT NOT NULL,
  prepared_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL
);
CREATE INDEX hr_payroll_final_pay_company ON hr_payroll_final_pay_preparations(company_id,status,created_at DESC);

CREATE TABLE hr_payroll_final_pay_events (
  id TEXT PRIMARY KEY,
  preparation_id TEXT NOT NULL REFERENCES hr_payroll_final_pay_preparations(id),
  task_id TEXT NOT NULL REFERENCES people_preparation_tasks(id),
  run_id TEXT NOT NULL REFERENCES hr_payroll_runs(id),
  company_id TEXT NOT NULL,
  action TEXT NOT NULL CHECK(action IN ('prepared','cancelled')),
  actor_id TEXT NOT NULL,
  task_version BIGINT NOT NULL,
  evidence_json TEXT NOT NULL CHECK(erpx_json_valid(evidence_json)),
  created_at BIGINT NOT NULL
);
CREATE INDEX hr_payroll_final_pay_history ON hr_payroll_final_pay_events(task_id,created_at,id);
CREATE TRIGGER hr_payroll_final_pay_event_immutable
  BEFORE UPDATE OR DELETE ON hr_payroll_final_pay_events
  FOR EACH ROW EXECUTE FUNCTION iam_immutable();
