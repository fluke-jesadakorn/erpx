-- Calendar reads authoritative sources. These tables own only new appointment/notice workflows.
CREATE UNIQUE INDEX hr_people_calendar_company_identity ON hr_people(company_id,id);
CREATE TABLE calendar_appointments (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, branch_id TEXT, department_id TEXT,
 organizer_person_id TEXT NOT NULL, title TEXT NOT NULL, description TEXT NOT NULL DEFAULT '',
 starts_at BIGINT NOT NULL, ends_at BIGINT NOT NULL CHECK(ends_at>starts_at), timezone TEXT NOT NULL,
 location TEXT NOT NULL DEFAULT '', meeting_url TEXT NOT NULL DEFAULT '', private BOOLEAN NOT NULL DEFAULT true,
 guests JSONB NOT NULL DEFAULT '[]', status TEXT NOT NULL CHECK(status IN ('scheduled','cancelled','completed','no_show')),
 version INTEGER NOT NULL DEFAULT 1, created_by TEXT NOT NULL, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL,
 UNIQUE(company_id,id), FOREIGN KEY(company_id,organizer_person_id) REFERENCES hr_people(company_id,id)
);
CREATE INDEX calendar_appointments_range ON calendar_appointments(company_id,starts_at,ends_at);
CREATE TABLE calendar_appointment_participants (
 company_id TEXT NOT NULL, appointment_id TEXT NOT NULL, person_id TEXT NOT NULL,
 PRIMARY KEY(appointment_id,person_id),
 FOREIGN KEY(company_id,appointment_id) REFERENCES calendar_appointments(company_id,id),
 FOREIGN KEY(company_id,person_id) REFERENCES hr_people(company_id,id)
);
CREATE INDEX calendar_participant_person ON calendar_appointment_participants(company_id,person_id,appointment_id);
CREATE TABLE calendar_announcements (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, branch_id TEXT, department_id TEXT,
 title TEXT NOT NULL, content TEXT NOT NULL, start_date TEXT NOT NULL, end_date TEXT NOT NULL CHECK(end_date>=start_date),
 status TEXT NOT NULL CHECK(status IN ('draft','published','withdrawn')), version INTEGER NOT NULL DEFAULT 1,
 created_by TEXT NOT NULL, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL
);
CREATE INDEX calendar_announcements_range ON calendar_announcements(company_id,start_date,end_date);
CREATE TABLE calendar_history (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, source TEXT NOT NULL, record_id TEXT NOT NULL,
 action TEXT NOT NULL, actor_id TEXT NOT NULL, version INTEGER NOT NULL, created_at BIGINT NOT NULL,
 UNIQUE(company_id,source,record_id,version)
);
CREATE INDEX calendar_history_record ON calendar_history(company_id,source,record_id,created_at);
-- Existing assignments and IT restrictions are deliberately retained. No inferred deadlines.
ALTER TABLE people_preparation_tasks ADD COLUMN due_date TEXT;
ALTER TABLE people_preparation_tasks ADD COLUMN responsibility TEXT NOT NULL DEFAULT '';
ALTER TABLE people_preparation_tasks ADD COLUMN privileged BOOLEAN NOT NULL DEFAULT true;
ALTER TABLE people_preparation_tasks ADD COLUMN completed_by TEXT;
ALTER TABLE people_preparation_tasks ADD COLUMN completed_at BIGINT;
CREATE INDEX people_preparation_due ON people_preparation_tasks(company_id,due_date) WHERE status='pending';
CREATE TRIGGER calendar_history_immutable BEFORE UPDATE OR DELETE ON calendar_history FOR EACH ROW EXECUTE FUNCTION iam_immutable();
INSERT INTO iam_permissions(id,definition) SELECT p.id,jsonb_build_object('id',p.id,'label',p.label,'module',p.module,'namespace','customer','availability','live','risk','normal','roles','["holding_owner","hr_admin","people_manager","operations_manager"]'::jsonb,'conditions','[]'::jsonb,'permittedScopes','["holding","branch","department"]'::jsonb) FROM (VALUES
 ('calendar.availability.read','Read availability without private details','people_payroll'),
 ('calendar.appointments.manage','Manage scoped appointments','people_payroll'),
 ('calendar.announcements.publish','Publish scoped announcements','collaboration')) p(id,label,module);
INSERT INTO iam_role_revisions
 SELECT r.company_id,r.id,r.published_revision+1,
 v.permissions_json || '["calendar.availability.read","calendar.appointments.manage","calendar.announcements.publish"]'::jsonb,
 v.grantable_json || CASE WHEN r.id IN ('holding_owner','hr_admin') THEN '["calendar.availability.read","calendar.appointments.manage","calendar.announcements.publish"]'::jsonb ELSE '[]'::jsonb END,
 v.scopes_json,'system',floor(extract(epoch FROM clock_timestamp())*1000)::bigint
 FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision
 WHERE r.company_id='*' AND r.id IN ('holding_owner','hr_admin','people_manager','operations_manager');
UPDATE iam_roles r SET published_revision=(SELECT max(revision) FROM iam_role_revisions v WHERE v.company_id=r.company_id AND v.role_id=r.id) WHERE r.company_id='*' AND r.id IN ('holding_owner','hr_admin','people_manager','operations_manager');
