ALTER TABLE finance_expenses DROP CONSTRAINT finance_expenses_workflow_stage_check;
ALTER TABLE finance_expenses ADD CONSTRAINT finance_expenses_workflow_stage_check
  CHECK (workflow_stage IN ('draft','department_approval','accounts_review','accounts_approval','gl_posting','payment','completed','cancelled'));

-- Append-only defaults: a new revision applies to new document routes only.
CREATE TABLE flow_defaults (
  revision BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  company_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  step_key TEXT NOT NULL,
  branch_id TEXT,
  department_id TEXT,
  primary_user_id TEXT NOT NULL,
  created_by TEXT NOT NULL,
  reason TEXT NOT NULL,
  created_at BIGINT NOT NULL
);
CREATE INDEX flow_defaults_lookup ON flow_defaults(company_id,kind,step_key,revision DESC);
CREATE TABLE flow_routes (
  company_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  record_id TEXT NOT NULL,
  version BIGINT NOT NULL DEFAULT 1,
  defaults_revision BIGINT NOT NULL,
  steps_json TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  PRIMARY KEY(company_id,kind,record_id)
);
CREATE TABLE flow_events (
  id TEXT PRIMARY KEY,
  command_id TEXT NOT NULL,
  company_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  record_id TEXT NOT NULL,
  step_key TEXT NOT NULL,
  action TEXT NOT NULL,
  actor_user_id TEXT NOT NULL,
  actor_name TEXT NOT NULL,
  reason TEXT NOT NULL DEFAULT '',
  record_version BIGINT NOT NULL,
  created_at BIGINT NOT NULL,
  UNIQUE(command_id,kind,record_id)
);
CREATE INDEX flow_events_record ON flow_events(company_id,kind,record_id,created_at);
