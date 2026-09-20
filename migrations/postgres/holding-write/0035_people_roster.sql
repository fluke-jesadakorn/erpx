CREATE TABLE people_roster_locks (company_id TEXT PRIMARY KEY, version BIGINT NOT NULL DEFAULT 1);
CREATE TABLE people_roster_templates (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL REFERENCES organization_nodes(id), department_id TEXT NOT NULL REFERENCES organization_nodes(id),
 name TEXT NOT NULL, start_time TEXT NOT NULL, end_time TEXT NOT NULL, capacity INTEGER NOT NULL CHECK(capacity BETWEEN 1 AND 100),
 active BOOLEAN NOT NULL DEFAULT true, version INTEGER NOT NULL DEFAULT 1, created_by TEXT NOT NULL, updated_at BIGINT NOT NULL
);
CREATE TABLE people_roster_weeks (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL REFERENCES organization_nodes(id), branch_id TEXT REFERENCES organization_nodes(id), department_id TEXT NOT NULL REFERENCES organization_nodes(id),
 week_start DATE NOT NULL CHECK(extract(isodow FROM week_start)=1), timezone TEXT NOT NULL,
 state TEXT NOT NULL DEFAULT 'draft' CHECK(state IN ('draft','published')), version INTEGER NOT NULL DEFAULT 1,
 created_by TEXT NOT NULL, published_by TEXT, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL,
 UNIQUE(company_id,department_id,week_start)
);
CREATE TABLE people_roster_shifts (
 id TEXT PRIMARY KEY, week_id TEXT NOT NULL REFERENCES people_roster_weeks(id), template_id TEXT REFERENCES people_roster_templates(id),
 name TEXT NOT NULL, local_date DATE NOT NULL, start_time TEXT NOT NULL, end_time TEXT NOT NULL,
 starts_at BIGINT NOT NULL, ends_at BIGINT NOT NULL CHECK(ends_at>starts_at), capacity INTEGER NOT NULL CHECK(capacity BETWEEN 1 AND 100),
 state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active','cancelled')), version INTEGER NOT NULL DEFAULT 1
);
CREATE TABLE people_roster_assignments (
 id TEXT PRIMARY KEY, shift_id TEXT NOT NULL REFERENCES people_roster_shifts(id), person_id TEXT NOT NULL REFERENCES hr_people(id), engagement_id TEXT NOT NULL REFERENCES hr_engagements(id),
 state TEXT NOT NULL DEFAULT 'assigned' CHECK(state IN ('assigned','released')), version INTEGER NOT NULL DEFAULT 1
);
CREATE UNIQUE INDEX people_roster_assignment_active ON people_roster_assignments(shift_id,person_id) WHERE state='assigned';
CREATE INDEX people_roster_person ON people_roster_assignments(person_id) WHERE state='assigned';
CREATE TABLE people_roster_requests (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL REFERENCES organization_nodes(id), department_id TEXT NOT NULL REFERENCES organization_nodes(id),
 kind TEXT NOT NULL CHECK(kind IN ('extra','move','swap')), requester_person_id TEXT NOT NULL REFERENCES hr_people(id), coworker_person_id TEXT REFERENCES hr_people(id),
 source_assignment_id TEXT REFERENCES people_roster_assignments(id), source_version INTEGER,
 target_shift_id TEXT NOT NULL REFERENCES people_roster_shifts(id), target_assignment_id TEXT REFERENCES people_roster_assignments(id), target_version INTEGER,
 state TEXT NOT NULL CHECK(state IN ('pending_coworker','pending_manager','approved','rejected','withdrawn','invalidated')),
 reason TEXT NOT NULL, decision_note TEXT NOT NULL DEFAULT '', accepted_by TEXT, decided_by TEXT,
 version INTEGER NOT NULL DEFAULT 1, created_by TEXT NOT NULL, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL,
 CHECK((kind='extra' AND source_assignment_id IS NULL) OR (kind!='extra' AND source_assignment_id IS NOT NULL)),
 CHECK((kind='swap' AND target_assignment_id IS NOT NULL AND coworker_person_id IS NOT NULL) OR (kind!='swap' AND target_assignment_id IS NULL AND coworker_person_id IS NULL))
);
CREATE INDEX people_roster_requests_department ON people_roster_requests(company_id,department_id,state);
CREATE TABLE people_roster_history (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, department_id TEXT NOT NULL, week_id TEXT REFERENCES people_roster_weeks(id), request_id TEXT REFERENCES people_roster_requests(id),
 record_id TEXT NOT NULL, action TEXT NOT NULL, actor_id TEXT NOT NULL, reason TEXT NOT NULL, evidence_json TEXT NOT NULL, created_at BIGINT NOT NULL
);
CREATE FUNCTION people_roster_history_immutable() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'Roster history is append-only'; END $$;
CREATE TRIGGER people_roster_history_immutable BEFORE UPDATE OR DELETE ON people_roster_history FOR EACH ROW EXECUTE FUNCTION people_roster_history_immutable();

INSERT INTO iam_permissions(id,definition) SELECT p.id,jsonb_build_object('id',p.id,'label',p.label,'module','people_payroll','namespace','customer','availability','live','risk','normal','roles',p.roles::jsonb,'conditions','[]'::jsonb,'permittedScopes','["holding","branch","department"]'::jsonb,'identityBased',p.id='self.roster') FROM (VALUES
 ('roster.read','View team rosters','["holding_owner","hr_admin","people_manager","operations_manager"]'),
 ('roster.manage','Create and publish team rosters','["holding_owner","hr_admin","people_manager","operations_manager"]'),
 ('roster.approve','Approve roster changes independently','["holding_owner","hr_admin","people_manager","operations_manager"]'),
 ('self.roster','Participate in own roster','[]')) AS p(id,label,roles);
INSERT INTO iam_role_revisions
 SELECT r.company_id,r.id,r.published_revision+1,
 v.permissions_json || '["self.roster"]'::jsonb || CASE WHEN r.id IN ('holding_owner','hr_admin','people_manager','operations_manager') THEN '["roster.read","roster.manage","roster.approve"]'::jsonb ELSE '[]'::jsonb END,
 v.grantable_json || CASE WHEN r.id IN ('holding_owner','hr_admin') THEN '["self.roster","roster.read","roster.manage","roster.approve"]'::jsonb ELSE '[]'::jsonb END,
 v.scopes_json,'system',floor(extract(epoch FROM clock_timestamp())*1000)::bigint
 FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.company_id='*';
UPDATE iam_roles r SET published_revision=(SELECT max(revision) FROM iam_role_revisions v WHERE v.company_id=r.company_id AND v.role_id=r.id) WHERE r.company_id='*';
