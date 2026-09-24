CREATE TABLE staff_project_milestones (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  project_id TEXT NOT NULL REFERENCES staff_projects(id) ON DELETE CASCADE,
  title TEXT NOT NULL CHECK(length(title) BETWEEN 2 AND 200),
  description TEXT NOT NULL DEFAULT '' CHECK(length(description) <= 10000),
  due_at BIGINT NOT NULL,
  owner_user_id TEXT NOT NULL,
  state TEXT NOT NULL DEFAULT 'planned' CHECK(state IN ('planned','in_progress','completed','cancelled')),
  version BIGINT NOT NULL DEFAULT 1 CHECK(version > 0),
  created_by TEXT NOT NULL,
  completed_at BIGINT,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL,
  CHECK((state='completed') = (completed_at IS NOT NULL))
);
CREATE INDEX staff_project_milestones_due_idx ON staff_project_milestones(company_id,due_at,project_id,state);

CREATE TABLE staff_project_milestone_events (
  id TEXT PRIMARY KEY,
  milestone_id TEXT NOT NULL REFERENCES staff_project_milestones(id) ON DELETE CASCADE,
  milestone_version BIGINT NOT NULL CHECK(milestone_version > 0),
  action TEXT NOT NULL CHECK(action IN ('created','revised','started','completed','reopened','cancelled')),
  actor_id TEXT NOT NULL,
  reason TEXT NOT NULL,
  snapshot_json JSONB NOT NULL CHECK(jsonb_typeof(snapshot_json)='object'),
  created_at BIGINT NOT NULL,
  UNIQUE(milestone_id,milestone_version)
);
CREATE FUNCTION staff_project_milestone_event_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION 'PROJECT_MILESTONE_EVENT_IMMUTABLE' USING ERRCODE='23514';
END $$;
CREATE TRIGGER staff_project_milestone_events_no_update
  BEFORE UPDATE OR DELETE ON staff_project_milestone_events
  FOR EACH ROW EXECUTE FUNCTION staff_project_milestone_event_immutable();

SELECT ensure_work_cache_coverage();
