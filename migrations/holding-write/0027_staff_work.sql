PRAGMA foreign_keys = ON;

CREATE TABLE staff_levels (
  id TEXT PRIMARY KEY, company_id TEXT NOT NULL, code TEXT NOT NULL, name TEXT NOT NULL,
  rank INTEGER NOT NULL, active INTEGER NOT NULL DEFAULT 1 CHECK(active IN (0,1)),
  version INTEGER NOT NULL DEFAULT 1, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
  UNIQUE(company_id,code)
) STRICT;
ALTER TABLE hr_engagements ADD COLUMN level_id TEXT REFERENCES staff_levels(id);
CREATE TABLE staff_level_history (
  id TEXT PRIMARY KEY, engagement_id TEXT NOT NULL REFERENCES hr_engagements(id),
  previous_level_id TEXT REFERENCES staff_levels(id), level_id TEXT NOT NULL REFERENCES staff_levels(id),
  actor_id TEXT NOT NULL, reason TEXT NOT NULL, command_id TEXT NOT NULL, created_at INTEGER NOT NULL
) STRICT;
CREATE INDEX staff_level_history_engagement ON staff_level_history(engagement_id,created_at);
CREATE TABLE department_heads (
  department_id TEXT PRIMARY KEY REFERENCES organization_nodes(id), company_id TEXT NOT NULL,
  person_id TEXT NOT NULL REFERENCES hr_people(id), version INTEGER NOT NULL DEFAULT 1,
  updated_by TEXT NOT NULL, updated_at INTEGER NOT NULL
) STRICT;

CREATE TABLE staff_projects (
  id TEXT PRIMARY KEY, company_id TEXT NOT NULL, branch_id TEXT, department_id TEXT,
  title TEXT NOT NULL, description TEXT NOT NULL DEFAULT '', owner_user_id TEXT NOT NULL,
  version INTEGER NOT NULL DEFAULT 1, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
) STRICT;
CREATE TABLE staff_project_members (
  project_id TEXT NOT NULL REFERENCES staff_projects(id), user_id TEXT NOT NULL,
  added_by TEXT NOT NULL, created_at INTEGER NOT NULL, PRIMARY KEY(project_id,user_id)
) STRICT;
CREATE TABLE routine_templates (
  id TEXT PRIMARY KEY, company_id TEXT NOT NULL, branch_id TEXT, department_id TEXT,
  title TEXT NOT NULL, description TEXT NOT NULL DEFAULT '', creator_user_id TEXT NOT NULL,
  assignee_user_id TEXT NOT NULL, project_id TEXT REFERENCES staff_projects(id),
  start_approvers_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(start_approvers_json)),
  completion_approvers_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(completion_approvers_json)),
  published INTEGER NOT NULL DEFAULT 0 CHECK(published IN (0,1)),
  version INTEGER NOT NULL DEFAULT 1, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
) STRICT;
CREATE TABLE routine_schedules (
  id TEXT PRIMARY KEY, template_id TEXT NOT NULL REFERENCES routine_templates(id), company_id TEXT NOT NULL,
  frequency TEXT NOT NULL CHECK(frequency IN ('daily','weekly','monthly')),
  timezone TEXT NOT NULL DEFAULT 'Asia/Bangkok', occurrence_time TEXT NOT NULL DEFAULT '09:00',
  start_date TEXT NOT NULL, end_date TEXT, weekday INTEGER CHECK(weekday BETWEEN 0 AND 6),
  month_day INTEGER CHECK(month_day BETWEEN 1 AND 31), due_offset_hours INTEGER NOT NULL DEFAULT 24 CHECK(due_offset_hours BETWEEN 0 AND 8760),
  state TEXT NOT NULL DEFAULT 'paused' CHECK(state IN ('active','paused','ended')),
  next_occurrence_at INTEGER, paused_at INTEGER, last_generated_at INTEGER, last_error TEXT,
  version INTEGER NOT NULL DEFAULT 1, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
) STRICT;
CREATE INDEX routine_schedule_due ON routine_schedules(state,next_occurrence_at);
CREATE TABLE routine_schedule_pauses (
  id TEXT PRIMARY KEY, schedule_id TEXT NOT NULL REFERENCES routine_schedules(id),
  started_at INTEGER NOT NULL, ended_at INTEGER
) STRICT;
CREATE INDEX routine_pause_schedule ON routine_schedule_pauses(schedule_id,started_at);
CREATE TABLE staff_tasks (
  id TEXT PRIMARY KEY, company_id TEXT NOT NULL, branch_id TEXT, department_id TEXT,
  title TEXT NOT NULL, description TEXT NOT NULL DEFAULT '', creator_user_id TEXT NOT NULL,
  assignee_user_id TEXT NOT NULL, due_at INTEGER NOT NULL, project_id TEXT REFERENCES staff_projects(id),
  completion_note TEXT NOT NULL DEFAULT '',
  start_approvers_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(start_approvers_json)),
  completion_approvers_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(completion_approvers_json)),
  state TEXT NOT NULL DEFAULT 'draft' CHECK(state IN ('draft','start_approval','ready','in_progress','completion_approval','rework','completed','cancelled')),
  blocked_reason TEXT, revision INTEGER NOT NULL DEFAULT 1, version INTEGER NOT NULL DEFAULT 1,
  template_id TEXT REFERENCES routine_templates(id), template_version INTEGER,
  schedule_id TEXT REFERENCES routine_schedules(id), occurrence_at INTEGER,
  created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
  UNIQUE(schedule_id,occurrence_at)
) STRICT;
CREATE INDEX staff_tasks_company ON staff_tasks(company_id,state,updated_at);
CREATE INDEX staff_tasks_assignee ON staff_tasks(company_id,assignee_user_id,state);
CREATE TABLE approval_instances (
  id TEXT PRIMARY KEY, company_id TEXT NOT NULL, branch_id TEXT, department_id TEXT,
  record_type TEXT NOT NULL CHECK(record_type IN ('expense','leave','task')),
  record_id TEXT NOT NULL, phase TEXT NOT NULL CHECK(phase IN ('request','start','completion')),
  cycle INTEGER NOT NULL, requester_user_id TEXT NOT NULL, assignee_user_id TEXT,
  state TEXT NOT NULL CHECK(state IN ('pending','blocked','approved','rejected','cancelled','superseded')),
  current_step INTEGER NOT NULL DEFAULT 0, blocked_reason TEXT, version INTEGER NOT NULL DEFAULT 1,
  created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
  UNIQUE(record_type,record_id,phase,cycle)
) STRICT;
CREATE UNIQUE INDEX approval_one_open ON approval_instances(record_type,record_id,phase) WHERE state IN ('pending','blocked');
CREATE INDEX approval_record ON approval_instances(company_id,record_type,record_id,created_at);
CREATE TABLE approval_steps (
  id TEXT PRIMARY KEY, instance_id TEXT NOT NULL REFERENCES approval_instances(id), ordinal INTEGER NOT NULL,
  approver_user_id TEXT NOT NULL, authority TEXT NOT NULL CHECK(authority IN ('named','department','hr_admin','holding_owner')),
  state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','approved','rejected')),
  UNIQUE(instance_id,ordinal)
) STRICT;
CREATE INDEX approval_user ON approval_steps(approver_user_id,instance_id);
CREATE TABLE approval_decisions (
  id TEXT PRIMARY KEY, instance_id TEXT NOT NULL REFERENCES approval_instances(id), step_id TEXT REFERENCES approval_steps(id),
  action TEXT NOT NULL CHECK(action IN ('approve','reject','cancel','revise','reroute','initialize')),
  actor_user_id TEXT NOT NULL, previous_approver_user_id TEXT, approver_user_id TEXT,
  reason TEXT NOT NULL, reviewed_version INTEGER NOT NULL, command_id TEXT NOT NULL,
  created_at INTEGER NOT NULL
) STRICT;
CREATE INDEX approval_decision_history ON approval_decisions(instance_id,created_at);
CREATE TABLE staff_notifications (
  id TEXT PRIMARY KEY, company_id TEXT NOT NULL, user_id TEXT NOT NULL, record_type TEXT NOT NULL,
  record_id TEXT NOT NULL, record_version INTEGER NOT NULL, message TEXT NOT NULL,
  created_at INTEGER NOT NULL, read_at INTEGER,
  UNIQUE(user_id,record_type,record_id,record_version,message)
) STRICT;
CREATE INDEX staff_notification_user ON staff_notifications(company_id,user_id,created_at);

ALTER TABLE leave_requests ADD COLUMN person_id TEXT REFERENCES hr_people(id);
ALTER TABLE leave_requests ADD COLUMN engagement_id TEXT REFERENCES hr_engagements(id);
UPDATE leave_requests SET person_id=(SELECT person_id FROM employees WHERE employees.id=leave_requests.employee_id);
UPDATE leave_requests SET engagement_id=(SELECT id FROM hr_engagements e WHERE e.person_id=leave_requests.person_id AND e.company_id=leave_requests.company_id ORDER BY CASE WHEN status='active' THEN 0 ELSE 1 END,start_date DESC,id LIMIT 1);
-- Historical final decisions remain untouched. Pending requests require a new, independent route.
INSERT INTO approval_instances(id,company_id,branch_id,department_id,record_type,record_id,phase,cycle,requester_user_id,state,blocked_reason,created_at,updated_at)
SELECT 'legacy-leave-'||id,company_id,branch_id,department_id,'leave',id,'request',1,created_by,'blocked','Route configuration required',updated_at,updated_at FROM leave_requests WHERE state='pending_approval';
INSERT INTO approval_instances(id,company_id,branch_id,department_id,record_type,record_id,phase,cycle,requester_user_id,state,blocked_reason,created_at,updated_at)
SELECT 'legacy-expense-'||x.id,x.company_id,x.branch_id,x.department_id,'expense',x.id,'request',1,COALESCE(p.user_id,x.created_by),'blocked','Route configuration required',x.updated_at,x.updated_at FROM finance_expenses x LEFT JOIN hr_people p ON p.id=x.employee_id WHERE x.status='in_review' AND x.cancelled_at IS NULL;
