CREATE TABLE staff_project_work_logs (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  project_id TEXT NOT NULL REFERENCES staff_projects(id) ON DELETE CASCADE,
  user_id TEXT NOT NULL,
  work_date DATE NOT NULL,
  minutes BIGINT NOT NULL CHECK(minutes BETWEEN 1 AND 1440),
  state TEXT NOT NULL DEFAULT 'submitted' CHECK(state IN ('submitted','approved','rejected')),
  version BIGINT NOT NULL DEFAULT 1 CHECK(version > 0),
  reviewed_by TEXT,
  reviewed_at BIGINT,
  rejection_reason TEXT NOT NULL DEFAULT '',
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL,
  UNIQUE(project_id,user_id,work_date),
  CHECK((state='approved') = (reviewed_by IS NOT NULL AND reviewed_at IS NOT NULL))
);
CREATE INDEX staff_project_work_logs_metric_idx ON staff_project_work_logs(company_id,project_id,work_date,state);

CREATE TABLE staff_project_work_log_events (
  id TEXT PRIMARY KEY,
  work_log_id TEXT NOT NULL REFERENCES staff_project_work_logs(id) ON DELETE CASCADE,
  work_log_version BIGINT NOT NULL CHECK(work_log_version > 0),
  action TEXT NOT NULL CHECK(action IN ('submitted','revised','approved','rejected')),
  actor_id TEXT NOT NULL,
  reason TEXT NOT NULL,
  snapshot_json JSONB NOT NULL CHECK(jsonb_typeof(snapshot_json)='object'),
  created_at BIGINT NOT NULL,
  UNIQUE(work_log_id,work_log_version)
);

CREATE TABLE staff_project_cost_allocations (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  project_id TEXT NOT NULL REFERENCES staff_projects(id) ON DELETE CASCADE,
  journal_line_id TEXT NOT NULL REFERENCES finance_journal_lines(id),
  amount_minor BIGINT NOT NULL CHECK(amount_minor > 0),
  state TEXT NOT NULL DEFAULT 'draft' CHECK(state IN ('draft','approved','rejected')),
  version BIGINT NOT NULL DEFAULT 1 CHECK(version > 0),
  created_by TEXT NOT NULL,
  approved_by TEXT,
  rejected_by TEXT,
  rejection_reason TEXT NOT NULL DEFAULT '',
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL,
  UNIQUE(journal_line_id,project_id),
  CHECK((state='approved') = (approved_by IS NOT NULL)),
  CHECK(state!='rejected' OR rejected_by IS NOT NULL)
);
CREATE INDEX staff_project_cost_allocations_project_idx ON staff_project_cost_allocations(company_id,project_id,state);

CREATE TABLE staff_project_cost_allocation_events (
  id TEXT PRIMARY KEY,
  allocation_id TEXT NOT NULL REFERENCES staff_project_cost_allocations(id) ON DELETE CASCADE,
  allocation_version BIGINT NOT NULL CHECK(allocation_version > 0),
  action TEXT NOT NULL CHECK(action IN ('created','revised','approved','rejected')),
  actor_id TEXT NOT NULL,
  reason TEXT NOT NULL,
  snapshot_json JSONB NOT NULL CHECK(jsonb_typeof(snapshot_json)='object'),
  created_at BIGINT NOT NULL,
  UNIQUE(allocation_id,allocation_version)
);

CREATE FUNCTION project_actual_event_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION 'PROJECT_ACTUAL_EVENT_IMMUTABLE' USING ERRCODE='23514';
END $$;
CREATE TRIGGER staff_project_work_log_events_no_update
  BEFORE UPDATE OR DELETE ON staff_project_work_log_events
  FOR EACH ROW EXECUTE FUNCTION project_actual_event_immutable();
CREATE TRIGGER staff_project_cost_allocation_events_no_update
  BEFORE UPDATE OR DELETE ON staff_project_cost_allocation_events
  FOR EACH ROW EXECUTE FUNCTION project_actual_event_immutable();

SELECT ensure_work_cache_coverage();
