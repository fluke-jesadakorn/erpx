-- Extend the existing project/milestone/task/actuals owners. Package identity is
-- the milestone ID; displayed WBS numbers are derived from sibling ordering.
ALTER TABLE staff_projects ADD COLUMN lifecycle_status TEXT NOT NULL DEFAULT 'active' CHECK(lifecycle_status IN ('active','handoff_pending','completed','cancelled'));
ALTER TABLE staff_project_milestones ADD COLUMN parent_milestone_id TEXT REFERENCES staff_project_milestones(id);
ALTER TABLE staff_project_milestones ADD COLUMN ordinal_position INTEGER NOT NULL DEFAULT 0 CHECK(ordinal_position BETWEEN 0 AND 1000000);
CREATE INDEX staff_project_package_parent ON staff_project_milestones(project_id,parent_milestone_id,ordinal_position,id);
CREATE TABLE staff_project_task_packages (
 task_id TEXT PRIMARY KEY REFERENCES staff_tasks(id),company_id TEXT NOT NULL,project_id TEXT NOT NULL REFERENCES staff_projects(id),
 milestone_id TEXT NOT NULL REFERENCES staff_project_milestones(id),source_task_version BIGINT NOT NULL,version BIGINT NOT NULL DEFAULT 1,
 linked_by TEXT NOT NULL,linked_at BIGINT NOT NULL
);
CREATE TABLE staff_project_package_actuals (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,project_id TEXT NOT NULL REFERENCES staff_projects(id),milestone_id TEXT NOT NULL REFERENCES staff_project_milestones(id),
 source_kind TEXT NOT NULL CHECK(source_kind IN ('work_log','cost_allocation')),source_id TEXT NOT NULL,source_version BIGINT NOT NULL,
 minutes BIGINT,amount_minor BIGINT,currency TEXT,linked_by TEXT NOT NULL,linked_at BIGINT NOT NULL,UNIQUE(source_kind,source_id),
 CHECK(source_kind='work_log' AND minutes>0 AND amount_minor IS NULL AND currency IS NULL OR source_kind='cost_allocation' AND minutes IS NULL AND amount_minor>0 AND currency IS NOT NULL)
);
CREATE TABLE staff_project_package_events (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,project_id TEXT NOT NULL REFERENCES staff_projects(id),milestone_id TEXT REFERENCES staff_project_milestones(id),
 action TEXT NOT NULL,actor_id TEXT NOT NULL,reason TEXT NOT NULL,snapshot_json JSONB NOT NULL,created_at BIGINT NOT NULL
);
CREATE TABLE staff_project_handoffs (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,project_id TEXT NOT NULL REFERENCES staff_projects(id),source_project_version BIGINT NOT NULL,recipient_id TEXT NOT NULL,
 status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','accepted','rejected','cancelled')),version BIGINT NOT NULL DEFAULT 1,
 snapshot_json JSONB NOT NULL,snapshot_sha256 TEXT NOT NULL CHECK(snapshot_sha256 ~ '^[a-f0-9]{64}$'),evidence TEXT NOT NULL,
 prepared_by TEXT NOT NULL,prepared_at BIGINT NOT NULL,decided_by TEXT,decided_at BIGINT,decision_reason TEXT,
 CHECK(status='pending' AND decided_by IS NULL AND decided_at IS NULL OR status<>'pending' AND decided_by IS NOT NULL AND decided_at IS NOT NULL)
);
CREATE UNIQUE INDEX staff_project_pending_handoff ON staff_project_handoffs(project_id) WHERE status='pending';
CREATE FUNCTION staff_project_closed_definition_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF OLD.lifecycle_status<>'active' AND (NEW.title,NEW.description,NEW.owner_user_id,NEW.company_id,NEW.branch_id,NEW.department_id)
  IS DISTINCT FROM (OLD.title,OLD.description,OLD.owner_user_id,OLD.company_id,OLD.branch_id,OLD.department_id)
 THEN RAISE EXCEPTION 'PROJECT_STRUCTURE_FROZEN' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER staff_project_closed_definition BEFORE UPDATE ON staff_projects FOR EACH ROW EXECUTE FUNCTION staff_project_closed_definition_guard();
CREATE FUNCTION staff_project_closed_members_guard() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE target_project TEXT;project_status TEXT;
BEGIN
 target_project:=CASE WHEN TG_OP='DELETE' THEN OLD.project_id ELSE NEW.project_id END;
 SELECT lifecycle_status INTO project_status FROM staff_projects WHERE id=target_project FOR UPDATE;
 IF project_status<>'active' THEN RAISE EXCEPTION 'PROJECT_STRUCTURE_FROZEN' USING ERRCODE='23514'; END IF;
 IF TG_OP='DELETE' THEN RETURN OLD; ELSE RETURN NEW; END IF;
END $$;
CREATE TRIGGER staff_project_closed_members BEFORE INSERT OR UPDATE OR DELETE ON staff_project_members FOR EACH ROW EXECUTE FUNCTION staff_project_closed_members_guard();
CREATE FUNCTION staff_project_structure_guard() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE parent_company TEXT;parent_project TEXT;parent_state TEXT;project_status TEXT;
BEGIN
 SELECT lifecycle_status INTO project_status FROM staff_projects WHERE id=NEW.project_id AND company_id=NEW.company_id FOR UPDATE;
 IF project_status IS NULL OR project_status<>'active' THEN RAISE EXCEPTION 'PROJECT_STRUCTURE_FROZEN' USING ERRCODE='23514'; END IF;
 IF NEW.parent_milestone_id IS NOT NULL THEN
  SELECT company_id,project_id,state INTO parent_company,parent_project,parent_state FROM staff_project_milestones WHERE id=NEW.parent_milestone_id;
  IF parent_company IS DISTINCT FROM NEW.company_id OR parent_project IS DISTINCT FROM NEW.project_id OR NEW.parent_milestone_id=NEW.id
  THEN RAISE EXCEPTION 'PROJECT_PACKAGE_PARENT_SCOPE_INVALID' USING ERRCODE='23514'; END IF;
  IF TG_OP='INSERT' OR NEW.parent_milestone_id IS DISTINCT FROM OLD.parent_milestone_id THEN
   IF parent_state NOT IN ('planned','in_progress') THEN RAISE EXCEPTION 'PROJECT_PACKAGE_PARENT_TERMINAL' USING ERRCODE='23514'; END IF;
  END IF;
  IF EXISTS(WITH RECURSIVE ancestors(id,parent_id,path) AS (
    SELECT m.id,m.parent_milestone_id,ARRAY[m.id] FROM staff_project_milestones m WHERE m.id=NEW.parent_milestone_id
    UNION ALL SELECT m.id,m.parent_milestone_id,a.path||m.id FROM ancestors a JOIN staff_project_milestones m ON m.id=a.parent_id WHERE NOT m.id=ANY(a.path)
   ) SELECT 1 FROM ancestors WHERE id=NEW.id OR cardinality(path)>30)
  THEN RAISE EXCEPTION 'PROJECT_PACKAGE_CYCLE_OR_DEPTH' USING ERRCODE='23514'; END IF;
 END IF;
 IF NEW.state IN ('completed','cancelled') AND EXISTS(WITH RECURSIVE children(id,state) AS (
   SELECT id,state FROM staff_project_milestones WHERE parent_milestone_id=NEW.id
   UNION ALL SELECT m.id,m.state FROM children c JOIN staff_project_milestones m ON m.parent_milestone_id=c.id
  ) SELECT 1 FROM children WHERE state NOT IN ('completed','cancelled'))
 THEN RAISE EXCEPTION 'PROJECT_PACKAGE_CHILDREN_INCOMPLETE' USING ERRCODE='23514'; END IF;
 IF NEW.state IN ('completed','cancelled') AND EXISTS(SELECT 1 FROM staff_project_task_packages b JOIN staff_tasks t ON t.id=b.task_id WHERE b.milestone_id=NEW.id AND t.state NOT IN ('completed','cancelled'))
 THEN RAISE EXCEPTION 'PROJECT_PACKAGE_TASKS_INCOMPLETE' USING ERRCODE='23514'; END IF;
 IF NEW.state IN ('planned','in_progress') AND NEW.parent_milestone_id IS NOT NULL AND parent_state IN ('completed','cancelled')
 THEN RAISE EXCEPTION 'PROJECT_PACKAGE_PARENT_TERMINAL' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER staff_project_structure_guard BEFORE INSERT OR UPDATE ON staff_project_milestones FOR EACH ROW EXECUTE FUNCTION staff_project_structure_guard();
CREATE FUNCTION staff_project_execution_graph_guard() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE target_project TEXT;
BEGIN
 IF TG_TABLE_NAME='staff_project_milestones' THEN
  IF TG_OP='UPDATE' THEN
   IF NEW.parent_milestone_id IS NOT DISTINCT FROM OLD.parent_milestone_id THEN RETURN NEW; END IF;
  END IF;
 END IF;
 target_project:=NEW.project_id;
 IF EXISTS(WITH RECURSIVE edges(predecessor,successor) AS (
   SELECT predecessor_milestone_id,successor_milestone_id FROM staff_project_milestone_dependencies WHERE project_id=target_project
   UNION SELECT id,parent_milestone_id FROM staff_project_milestones WHERE project_id=target_project AND parent_milestone_id IS NOT NULL
  ),reachable(origin,target) AS (
   SELECT predecessor,successor FROM edges
   UNION SELECT r.origin,e.successor FROM reachable r JOIN edges e ON e.predecessor=r.target
  ) SELECT 1 FROM reachable WHERE origin=target)
 THEN RAISE EXCEPTION 'PROJECT_EXECUTION_GRAPH_CYCLE' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE CONSTRAINT TRIGGER staff_project_execution_graph_package AFTER INSERT OR UPDATE ON staff_project_milestones DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION staff_project_execution_graph_guard();
CREATE CONSTRAINT TRIGGER staff_project_execution_graph_dependency AFTER INSERT OR UPDATE ON staff_project_milestone_dependencies DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION staff_project_execution_graph_guard();
CREATE FUNCTION staff_project_task_package_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NOT EXISTS(SELECT 1 FROM staff_tasks t JOIN staff_project_milestones m ON m.id=NEW.milestone_id JOIN staff_projects p ON p.id=m.project_id
  WHERE t.id=NEW.task_id AND t.company_id=NEW.company_id AND t.project_id=NEW.project_id AND m.company_id=t.company_id AND m.project_id=t.project_id
   AND p.lifecycle_status='active' AND m.state IN ('planned','in_progress') AND t.version=NEW.source_task_version)
 THEN RAISE EXCEPTION 'PROJECT_PACKAGE_TASK_SOURCE_INVALID' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER staff_project_task_package_guard BEFORE INSERT OR UPDATE ON staff_project_task_packages FOR EACH ROW EXECUTE FUNCTION staff_project_task_package_guard();
CREATE FUNCTION staff_project_linked_task_guard() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE project_status TEXT;
BEGIN
 IF EXISTS(SELECT 1 FROM staff_project_task_packages b WHERE b.task_id=NEW.id AND (b.company_id<>NEW.company_id OR b.project_id IS DISTINCT FROM NEW.project_id))
 THEN RAISE EXCEPTION 'PROJECT_PACKAGE_TASK_SOURCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 IF TG_OP='UPDATE' AND OLD.project_id IS NOT NULL AND OLD.project_id IS DISTINCT FROM NEW.project_id THEN
  SELECT lifecycle_status INTO project_status FROM staff_projects WHERE id=OLD.project_id FOR UPDATE;
  IF project_status<>'active' THEN RAISE EXCEPTION 'PROJECT_STRUCTURE_FROZEN' USING ERRCODE='23514'; END IF;
 END IF;
 IF NEW.project_id IS NOT NULL THEN
  SELECT lifecycle_status INTO project_status FROM staff_projects WHERE id=NEW.project_id FOR UPDATE;
  IF project_status<>'active' THEN RAISE EXCEPTION 'PROJECT_STRUCTURE_FROZEN' USING ERRCODE='23514'; END IF;
 END IF;
 IF NEW.state NOT IN ('completed','cancelled') AND EXISTS(SELECT 1 FROM staff_project_task_packages b JOIN staff_project_milestones m ON m.id=b.milestone_id WHERE b.task_id=NEW.id AND m.state IN ('completed','cancelled'))
 THEN RAISE EXCEPTION 'PROJECT_PACKAGE_TERMINAL' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER staff_project_linked_task_guard BEFORE INSERT OR UPDATE ON staff_tasks FOR EACH ROW EXECUTE FUNCTION staff_project_linked_task_guard();
CREATE FUNCTION staff_project_package_actual_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NOT EXISTS(SELECT 1 FROM staff_project_milestones m JOIN staff_projects p ON p.id=m.project_id AND p.company_id=m.company_id
  WHERE m.id=NEW.milestone_id AND m.project_id=NEW.project_id AND m.company_id=NEW.company_id AND p.lifecycle_status='active')
 THEN RAISE EXCEPTION 'PROJECT_PACKAGE_ACTUAL_SCOPE_INVALID' USING ERRCODE='23514'; END IF;
 IF NEW.source_kind='work_log' THEN
  IF NOT EXISTS(SELECT 1 FROM staff_project_work_logs l WHERE l.id=NEW.source_id AND l.company_id=NEW.company_id AND l.project_id=NEW.project_id AND l.version=NEW.source_version AND l.state='approved' AND l.minutes=NEW.minutes)
  THEN RAISE EXCEPTION 'PROJECT_PACKAGE_TIME_SOURCE_INVALID' USING ERRCODE='23514'; END IF;
 ELSE
  IF NOT EXISTS(SELECT 1 FROM staff_project_cost_allocations c JOIN finance_journal_lines l ON l.id=c.journal_line_id JOIN finance_journals j ON j.id=l.journal_id
   WHERE c.id=NEW.source_id AND c.company_id=NEW.company_id AND c.project_id=NEW.project_id AND c.version=NEW.source_version AND c.state='approved' AND j.status='posted' AND c.amount_minor=NEW.amount_minor AND j.currency=NEW.currency)
  THEN RAISE EXCEPTION 'PROJECT_PACKAGE_COST_SOURCE_INVALID' USING ERRCODE='23514'; END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER staff_project_package_actual_guard BEFORE INSERT ON staff_project_package_actuals FOR EACH ROW EXECUTE FUNCTION staff_project_package_actual_guard();
CREATE FUNCTION staff_project_package_evidence_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'PROJECT_PACKAGE_EVIDENCE_IMMUTABLE' USING ERRCODE='23514'; END $$;
CREATE TRIGGER staff_project_package_actual_immutable BEFORE UPDATE OR DELETE ON staff_project_package_actuals FOR EACH ROW EXECUTE FUNCTION staff_project_package_evidence_immutable();
CREATE TRIGGER staff_project_package_event_immutable BEFORE UPDATE OR DELETE ON staff_project_package_events FOR EACH ROW EXECUTE FUNCTION staff_project_package_evidence_immutable();
CREATE FUNCTION staff_project_closed_actual_guard() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE project_status TEXT;
BEGIN
 SELECT lifecycle_status INTO project_status FROM staff_projects WHERE id=NEW.project_id FOR UPDATE;
 IF project_status<>'active' THEN RAISE EXCEPTION 'PROJECT_STRUCTURE_FROZEN' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER staff_project_closed_time_guard BEFORE INSERT OR UPDATE ON staff_project_work_logs FOR EACH ROW EXECUTE FUNCTION staff_project_closed_actual_guard();
CREATE TRIGGER staff_project_closed_cost_guard BEFORE INSERT OR UPDATE ON staff_project_cost_allocations FOR EACH ROW EXECUTE FUNCTION staff_project_closed_actual_guard();
CREATE FUNCTION staff_project_handoff_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='DELETE' OR (NEW.id,NEW.company_id,NEW.project_id,NEW.source_project_version,NEW.recipient_id,NEW.snapshot_json,NEW.snapshot_sha256,NEW.evidence,NEW.prepared_by,NEW.prepared_at)
  IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.project_id,OLD.source_project_version,OLD.recipient_id,OLD.snapshot_json,OLD.snapshot_sha256,OLD.evidence,OLD.prepared_by,OLD.prepared_at)
 THEN RAISE EXCEPTION 'PROJECT_HANDOFF_SOURCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 IF OLD.status<>'pending' THEN RAISE EXCEPTION 'PROJECT_HANDOFF_DECISION_IMMUTABLE' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER staff_project_handoff_immutable BEFORE UPDATE OR DELETE ON staff_project_handoffs FOR EACH ROW EXECUTE FUNCTION staff_project_handoff_immutable();
DO $$ DECLARE t TEXT; BEGIN FOREACH t IN ARRAY ARRAY['staff_project_task_packages','staff_project_package_actuals','staff_project_package_events','staff_project_handoffs'] LOOP
 INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;
 EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
 EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);
END LOOP;END $$;
