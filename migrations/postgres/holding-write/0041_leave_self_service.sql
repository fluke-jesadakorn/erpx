-- Company-scoped, engagement-linked leave self-service.
ALTER TABLE approval_instances DROP CONSTRAINT approval_instances_record_type_check;
ALTER TABLE approval_instances ADD CONSTRAINT approval_instances_record_type_check CHECK(record_type IN ('expense','leave','leave_cancellation','task'));
CREATE TABLE leave_types (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, code TEXT NOT NULL, name TEXT NOT NULL,
 description TEXT NOT NULL DEFAULT '', tracked BOOLEAN NOT NULL DEFAULT true,
 evidence_after_units NUMERIC(8,2), evidence_note TEXT NOT NULL DEFAULT '', private_evidence BOOLEAN NOT NULL DEFAULT true,
 active BOOLEAN NOT NULL DEFAULT true, effective_from TEXT NOT NULL, effective_to TEXT,
 created_by TEXT NOT NULL, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL,
 UNIQUE(company_id,code), UNIQUE(company_id,id), CHECK(effective_to IS NULL OR effective_to>=effective_from)
);
CREATE TABLE leave_policy_versions (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, policy_key TEXT NOT NULL, version INTEGER NOT NULL,
 name TEXT NOT NULL, status TEXT NOT NULL CHECK(status IN ('draft','active','retired')),
 effective_from TEXT NOT NULL, effective_to TEXT, timezone TEXT NOT NULL DEFAULT 'UTC',
 workweek_json JSONB NOT NULL DEFAULT '[1,2,3,4,5]', allow_negative BOOLEAN NOT NULL DEFAULT false,
 created_by TEXT NOT NULL, created_at BIGINT NOT NULL,
 UNIQUE(company_id,policy_key,version), UNIQUE(company_id,id),
 CHECK(effective_to IS NULL OR effective_to>=effective_from)
);
CREATE TABLE leave_policy_rules (
 policy_version_id TEXT NOT NULL REFERENCES leave_policy_versions(id),
 leave_type_id TEXT NOT NULL REFERENCES leave_types(id),
 grant_kind TEXT NOT NULL CHECK(grant_kind IN ('annual','period','accrual','untracked')),
 grant_units NUMERIC(8,2) NOT NULL DEFAULT 0, accrual_units NUMERIC(8,2) NOT NULL DEFAULT 0,
 accrual_period TEXT CHECK(accrual_period IN ('monthly','quarterly','annual')),
 carryover_limit NUMERIC(8,2), PRIMARY KEY(policy_version_id,leave_type_id)
);
CREATE TABLE leave_holiday_calendars (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, name TEXT NOT NULL, timezone TEXT NOT NULL,
 active BOOLEAN NOT NULL DEFAULT true, created_by TEXT NOT NULL, created_at BIGINT NOT NULL,
 UNIQUE(company_id,name), UNIQUE(company_id,id)
);
CREATE TABLE leave_holidays (
 id TEXT PRIMARY KEY, calendar_id TEXT NOT NULL REFERENCES leave_holiday_calendars(id),
 holiday_date TEXT NOT NULL, name TEXT NOT NULL, UNIQUE(calendar_id,holiday_date)
);
CREATE TABLE leave_policy_assignments (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, engagement_id TEXT NOT NULL REFERENCES hr_engagements(id),
 policy_version_id TEXT NOT NULL REFERENCES leave_policy_versions(id), holiday_calendar_id TEXT REFERENCES leave_holiday_calendars(id),
 effective_from TEXT NOT NULL, effective_to TEXT, created_by TEXT NOT NULL, created_at BIGINT NOT NULL,
 UNIQUE(company_id,id), CHECK(effective_to IS NULL OR effective_to>=effective_from)
);
CREATE INDEX leave_policy_assignment_effective ON leave_policy_assignments(company_id,engagement_id,effective_from,effective_to);
CREATE TABLE leave_balance_entries (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, engagement_id TEXT NOT NULL REFERENCES hr_engagements(id),
 leave_type_id TEXT NOT NULL REFERENCES leave_types(id), request_id TEXT REFERENCES leave_requests(id),
 kind TEXT NOT NULL CHECK(kind IN ('opening','grant','accrual','carryover','adjustment','reservation','reservation_release','usage','usage_reversal')),
 units NUMERIC(8,2) NOT NULL, effective_date TEXT NOT NULL, command_key TEXT NOT NULL,
 note TEXT NOT NULL DEFAULT '', created_by TEXT NOT NULL, created_at BIGINT NOT NULL,
 UNIQUE(company_id,command_key,kind,leave_type_id)
);
CREATE INDEX leave_balance_account ON leave_balance_entries(company_id,engagement_id,leave_type_id,effective_date);
CREATE FUNCTION leave_balance_entries_immutable() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'Leave balance entries are append-only'; END $$;
CREATE TRIGGER leave_balance_entries_immutable BEFORE UPDATE OR DELETE ON leave_balance_entries FOR EACH ROW EXECUTE FUNCTION leave_balance_entries_immutable();

ALTER TABLE leave_requests ADD COLUMN leave_type_id TEXT REFERENCES leave_types(id);
ALTER TABLE leave_requests ADD COLUMN end_date TEXT;
ALTER TABLE leave_requests ADD COLUMN start_segment TEXT NOT NULL DEFAULT 'full' CHECK(start_segment IN ('full','first_half','second_half'));
ALTER TABLE leave_requests ADD COLUMN end_segment TEXT NOT NULL DEFAULT 'full' CHECK(end_segment IN ('full','first_half','second_half'));
ALTER TABLE leave_requests ADD COLUMN policy_version_id TEXT REFERENCES leave_policy_versions(id);
ALTER TABLE leave_requests ADD COLUMN calculated_units NUMERIC(8,2);
ALTER TABLE leave_requests ADD COLUMN calculation_snapshot JSONB;
ALTER TABLE leave_requests ADD COLUMN cancellation_state TEXT CHECK(cancellation_state IN ('pending_review','approved','rejected'));
CREATE TABLE leave_day_segments (
 id TEXT PRIMARY KEY, request_id TEXT NOT NULL REFERENCES leave_requests(id), company_id TEXT NOT NULL,
 leave_date TEXT NOT NULL, segment TEXT NOT NULL CHECK(segment IN ('full','first_half','second_half')),
 units NUMERIC(3,2) NOT NULL CHECK(units IN (0.5,1)), source TEXT NOT NULL CHECK(source IN ('roster','workweek')),
 UNIQUE(request_id,leave_date)
);
CREATE INDEX leave_segments_company_date ON leave_day_segments(company_id,leave_date);
CREATE TABLE leave_cancellation_reviews (
 id TEXT PRIMARY KEY, request_id TEXT NOT NULL REFERENCES leave_requests(id), company_id TEXT NOT NULL,
 state TEXT NOT NULL DEFAULT 'pending_approval' CHECK(state IN ('pending_approval','approved','rejected')),
 reason TEXT NOT NULL, version INTEGER NOT NULL DEFAULT 1, requested_by TEXT NOT NULL,
 decided_by TEXT, decision_note TEXT, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL,
 UNIQUE(request_id,state)
);

-- Existing history stays usable, but is explicitly untracked rather than receiving invented allowance.
INSERT INTO leave_types(id,company_id,code,name,description,tracked,effective_from,created_by,created_at,updated_at)
 SELECT 'leave-type-legacy-'||company_id,company_id,'legacy-annual','Legacy annual leave','Migrated leave requests and balances.',false,'1900-01-01','migration',0,0
 FROM (SELECT DISTINCT company_id FROM leave_requests UNION SELECT DISTINCT e.company_id FROM hr_leave_balances b JOIN hr_engagements e ON e.id=b.engagement_id) c
 ON CONFLICT(company_id,code) DO NOTHING;
INSERT INTO leave_policy_versions(id,company_id,policy_key,version,name,status,effective_from,timezone,workweek_json,created_by,created_at)
 SELECT 'leave-policy-legacy-'||company_id,company_id,'legacy-untracked',1,'Legacy untracked leave','active','1900-01-01','UTC','[1,2,3,4,5]'::jsonb,'migration',0
 FROM (SELECT DISTINCT company_id FROM leave_types) c ON CONFLICT(company_id,policy_key,version) DO NOTHING;
INSERT INTO leave_policy_rules(policy_version_id,leave_type_id,grant_kind)
 SELECT p.id,t.id,'untracked' FROM leave_policy_versions p JOIN leave_types t ON t.company_id=p.company_id AND t.code='legacy-annual' WHERE p.policy_key='legacy-untracked'
 ON CONFLICT DO NOTHING;
UPDATE leave_requests r SET leave_type_id=t.id,end_date=(r.start_date::date+(r.days::int-1))::text,calculated_units=r.days,calculation_snapshot=jsonb_build_object('source','legacy','migrated',true)
 FROM leave_types t WHERE t.company_id=r.company_id AND t.code='legacy-annual' AND r.leave_type_id IS NULL;
INSERT INTO leave_balance_entries(id,company_id,engagement_id,leave_type_id,kind,units,effective_date,command_key,note,created_by,created_at)
 SELECT 'leave-opening-'||b.id,e.company_id,b.engagement_id,t.id,'opening',b.opening_days+b.accrued_days+b.carried_days-b.used_days,(b.year::text||'-01-01'),'legacy-balance-'||b.id,'Imported from the legacy balance projection.','migration',0
 FROM hr_leave_balances b JOIN hr_engagements e ON e.id=b.engagement_id JOIN leave_types t ON t.company_id=e.company_id AND t.code='legacy-annual'
 ON CONFLICT DO NOTHING;

INSERT INTO iam_permissions(id,definition) VALUES
 ('leave.self',jsonb_build_object('id','leave.self','label','Use personal leave self-service','module','people_payroll','namespace','customer','availability','live','risk','normal','roles','[]'::jsonb,'conditions','[]'::jsonb,'permittedScopes','["holding","branch","department"]'::jsonb,'identityBased',true)),
 ('leave.policy.manage',jsonb_build_object('id','leave.policy.manage','label','Manage company leave policies','module','people_payroll','namespace','customer','availability','live','risk','elevated','roles','["holding_owner","hr_admin"]'::jsonb,'conditions','[]'::jsonb,'permittedScopes','["holding"]'::jsonb,'identityBased',false))
 ON CONFLICT(id) DO NOTHING;
INSERT INTO iam_role_revisions
 SELECT r.company_id,r.id,r.published_revision+1,
 v.permissions_json || '["leave.self"]'::jsonb || CASE WHEN r.id IN ('holding_owner','hr_admin') THEN '["leave.policy.manage"]'::jsonb ELSE '[]'::jsonb END,
 v.grantable_json || CASE WHEN r.id IN ('holding_owner','hr_admin') THEN '["leave.self","leave.policy.manage"]'::jsonb ELSE '[]'::jsonb END,
 v.scopes_json,'system',floor(extract(epoch FROM clock_timestamp())*1000)::bigint
 FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision
 WHERE r.company_id='*';
UPDATE iam_roles r SET published_revision=(SELECT max(revision) FROM iam_role_revisions v WHERE v.company_id=r.company_id AND v.role_id=r.id) WHERE r.company_id='*';
