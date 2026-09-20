-- Positions are HR records. Publishing their access recipe never grants personal access.
ALTER TABLE organization_positions ADD COLUMN requirements TEXT NOT NULL DEFAULT '';
CREATE TABLE people_access_packages (
  company_id TEXT NOT NULL, id TEXT NOT NULL, version INTEGER NOT NULL,
  published_revision INTEGER NOT NULL DEFAULT 0, definition JSONB NOT NULL,
  PRIMARY KEY(company_id,id)
);
CREATE TABLE people_package_revisions (
  company_id TEXT NOT NULL, package_id TEXT NOT NULL, revision INTEGER NOT NULL,
  definition JSONB NOT NULL, created_by TEXT NOT NULL, created_at BIGINT NOT NULL,
  PRIMARY KEY(company_id,package_id,revision),
  FOREIGN KEY(company_id,package_id) REFERENCES people_access_packages(company_id,id)
);
CREATE TRIGGER people_revision_immutable BEFORE UPDATE OR DELETE ON people_package_revisions FOR EACH ROW EXECUTE FUNCTION iam_immutable();
CREATE TABLE people_position_packages (
  company_id TEXT NOT NULL, position_id TEXT NOT NULL, package_id TEXT NOT NULL, revision INTEGER NOT NULL,
  PRIMARY KEY(company_id,position_id),
  FOREIGN KEY(company_id,position_id) REFERENCES organization_positions(company_id,id),
  FOREIGN KEY(company_id,package_id,revision) REFERENCES people_package_revisions(company_id,package_id,revision)
);
CREATE TABLE people_lifecycle_settings (
  company_id TEXT PRIMARY KEY, version INTEGER NOT NULL, definition JSONB NOT NULL
);
CREATE TABLE people_lifecycle_cases (
  id TEXT PRIMARY KEY, company_id TEXT NOT NULL, person_id TEXT NOT NULL, engagement_id TEXT NOT NULL REFERENCES hr_engagements(id),
  kind TEXT NOT NULL CHECK(kind IN ('onboarding','transfer','offboarding','suspend')),
  status TEXT NOT NULL CHECK(status IN ('draft','waiting_it','pending_review','scheduled','completed','cancelled','blocked')),
  version INTEGER NOT NULL, effective_at BIGINT NOT NULL, package_id TEXT, package_revision INTEGER,
  attempts INTEGER NOT NULL DEFAULT 0, next_attempt_at BIGINT NOT NULL DEFAULT 0, last_error TEXT,
  definition JSONB NOT NULL, review_snapshot JSONB NOT NULL DEFAULT '{}', authorizing_assignment_id TEXT REFERENCES assignments(id), created_by TEXT NOT NULL, updated_at BIGINT NOT NULL,
  FOREIGN KEY(company_id,package_id,package_revision) REFERENCES people_package_revisions(company_id,package_id,revision)
);
CREATE UNIQUE INDEX people_case_inflight ON people_lifecycle_cases(company_id,person_id) WHERE status IN ('waiting_it','pending_review','scheduled');
CREATE INDEX people_case_due ON people_lifecycle_cases(effective_at) WHERE status='scheduled';
CREATE TABLE people_preparation_tasks (
  id TEXT PRIMARY KEY, company_id TEXT NOT NULL, position_id TEXT, case_id TEXT REFERENCES people_lifecycle_cases(id),
  kind TEXT NOT NULL, title TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','completed','cancelled')),
  assignee_id TEXT, version INTEGER NOT NULL DEFAULT 1, updated_at BIGINT NOT NULL,
  FOREIGN KEY(company_id,position_id) REFERENCES organization_positions(company_id,id)
);
CREATE UNIQUE INDEX people_position_preparation ON people_preparation_tasks(company_id,position_id) WHERE kind='position';
CREATE TABLE people_lifecycle_grants (
  id TEXT PRIMARY KEY, company_id TEXT NOT NULL, case_id TEXT NOT NULL REFERENCES people_lifecycle_cases(id),
  user_id TEXT NOT NULL, grant_index INTEGER NOT NULL, definition JSONB NOT NULL,
  review_id TEXT REFERENCES iam_reviews(id), assignment_id TEXT REFERENCES assignments(id),
  UNIQUE(case_id,user_id,grant_index)
);
-- A cutoff is effective by wall clock at authorization, independently of workers.
CREATE TABLE people_access_cutoffs (
  company_id TEXT NOT NULL, person_id TEXT NOT NULL, case_id TEXT NOT NULL REFERENCES people_lifecycle_cases(id),
  effective_at BIGINT NOT NULL, cancelled_at BIGINT, restored_at BIGINT,
  PRIMARY KEY(company_id,person_id,case_id)
);
CREATE OR REPLACE FUNCTION people_person_user(company_key TEXT,person_key TEXT,user_key TEXT) RETURNS BOOLEAN LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT 1 FROM hr_people WHERE company_id=company_key AND id=person_key AND user_id=user_key)
 OR EXISTS(SELECT 1 FROM iam_members WHERE company_id=company_key AND person_id=person_key AND user_id=user_key); $$;
CREATE OR REPLACE FUNCTION iam_principal_active(user_key TEXT,company_key TEXT,at_time BIGINT) RETURNS BOOLEAN LANGUAGE sql STABLE AS $$
 SELECT NOT EXISTS(SELECT 1 FROM iam_members m LEFT JOIN iam_identities i ON i.id=m.identity_id AND i.company_id=m.company_id LEFT JOIN iam_connections c ON c.id=m.connection_id AND c.company_id=m.company_id WHERE m.user_id=user_key AND m.company_id=company_key AND (m.status!='active' OR m.login_policy='directory' AND (i.enabled IS DISTINCT FROM true OR c.status IS DISTINCT FROM 'active' OR COALESCE(i.observed_at,0)<=at_time-300000)))
 AND NOT EXISTS(SELECT 1 FROM people_access_cutoffs x WHERE x.company_id=company_key AND x.cancelled_at IS NULL AND x.effective_at<=at_time AND (x.restored_at IS NULL OR x.restored_at>at_time) AND people_person_user(company_key,x.person_id,user_key)); $$;
CREATE FUNCTION people_lifecycle_grant_active(grant_key TEXT,at_time BIGINT) RETURNS BOOLEAN LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT 1 FROM people_lifecycle_grants g JOIN people_lifecycle_cases c ON c.id=g.case_id
 WHERE g.id=grant_key AND c.status IN ('scheduled','completed') AND c.effective_at<=at_time
 AND EXISTS(SELECT 1 FROM hr_engagements e JOIN hr_people h ON h.id=e.person_id AND h.company_id=e.company_id JOIN organization_positions p ON p.company_id=e.company_id AND p.id=c.definition->>'positionId' WHERE e.id=c.engagement_id AND e.status!='ended' AND h.status='active' AND h.merged_into_id IS NULL AND p.active=1 AND p.level_id=c.review_snapshot->>'levelId' AND p.branch_id IS NOT DISTINCT FROM c.review_snapshot->>'positionBranch' AND p.department_id IS NOT DISTINCT FROM c.review_snapshot->>'positionDepartment' AND (c.status='completed' AND e.position_id=p.id OR c.status='scheduled' AND e.version=(c.review_snapshot->>'engagementVersion')::integer))
 AND EXISTS(SELECT 1 FROM assignments a WHERE a.id=c.authorizing_assignment_id AND a.revoked_at IS NULL AND a.valid_from<=at_time AND a.valid_until>at_time AND iam_principal_active(a.user_id,a.company_id,at_time))
 AND NOT EXISTS(SELECT 1 FROM people_lifecycle_cases newer WHERE newer.company_id=c.company_id AND newer.person_id=c.person_id AND newer.id<>c.id AND newer.kind IN ('transfer','offboarding','suspend') AND newer.status IN ('scheduled','completed') AND newer.effective_at>=c.effective_at AND newer.effective_at<=at_time)); $$;
ALTER TABLE assignments DROP CONSTRAINT assignments_source_check;
ALTER TABLE assignments ADD CONSTRAINT assignments_source_check CHECK(source IN ('legacy','manual','directory','emergency','policy','lifecycle'));

-- Existing positions enter one shared, unassigned IT queue without changing access.
INSERT INTO people_preparation_tasks(id,company_id,position_id,kind,title,updated_at)
 SELECT 'position-prep-'||id,company_id,id,'position','Prepare access · '||title,floor(extract(epoch FROM clock_timestamp())*1000)::bigint FROM organization_positions WHERE active=1;
CREATE FUNCTION people_position_preparation_changed() RETURNS trigger LANGUAGE plpgsql AS $$
 BEGIN
 INSERT INTO people_preparation_tasks(id,company_id,position_id,kind,title,assignee_id,updated_at)
 VALUES('position-prep-'||NEW.id,NEW.company_id,NEW.id,'position','Prepare access · '||NEW.title,(SELECT definition->>'itAssigneeId' FROM people_lifecycle_settings WHERE company_id=NEW.company_id),floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
 ON CONFLICT(id) DO UPDATE SET title=excluded.title,updated_at=excluded.updated_at;
 RETURN NULL;
 END; $$;
CREATE TRIGGER people_position_preparation AFTER INSERT OR UPDATE OF title ON organization_positions FOR EACH ROW EXECUTE FUNCTION people_position_preparation_changed();

CREATE OR REPLACE FUNCTION iam_assignment_active(grant_id TEXT,at_time BIGINT) RETURNS BOOLEAN LANGUAGE sql STABLE AS $$
 WITH RECURSIVE chain AS (
 SELECT a.id,a.user_id,a.role,a.company_id,a.parent_id,a.revoked_at,a.valid_from,a.valid_until,a.source,a.source_id,ARRAY[a.id] AS path,false AS cycle FROM assignments a WHERE a.id=grant_id
 UNION ALL SELECT p.id,p.user_id,p.role,p.company_id,p.parent_id,p.revoked_at,p.valid_from,p.valid_until,p.source,p.source_id,c.path||p.id,p.id=ANY(c.path) FROM assignments p JOIN chain c ON p.id=c.parent_id WHERE NOT c.cycle AND cardinality(c.path)<32)
 SELECT EXISTS(SELECT 1 FROM chain) AND NOT EXISTS(SELECT 1 FROM chain WHERE revoked_at IS NOT NULL OR valid_from>at_time OR valid_until<=at_time OR cycle OR cardinality(path)>=32 OR NOT iam_principal_active(user_id,company_id,at_time) OR source='policy' AND NOT um_candidate_eligible(source_id,at_time) OR source='lifecycle' AND NOT people_lifecycle_grant_active(source_id,at_time))
 AND EXISTS(SELECT 1 FROM chain WHERE parent_id IS NULL)
 AND NOT EXISTS(SELECT 1 FROM chain ch JOIN chain p ON p.id=ch.parent_id
 LEFT JOIN iam_roles cr ON cr.id=ch.role AND cr.company_id IN ('*',ch.company_id) AND cr.status='active'
 LEFT JOIN iam_role_revisions cv ON cv.company_id=cr.company_id AND cv.role_id=cr.id AND cv.revision=cr.published_revision
 LEFT JOIN iam_roles pr ON pr.id=p.role AND pr.company_id IN ('*',p.company_id) AND pr.status='active'
 LEFT JOIN iam_role_revisions pv ON pv.company_id=pr.company_id AND pv.role_id=pr.id AND pv.revision=pr.published_revision
 WHERE ch.company_id!=p.company_id OR ch.valid_until>p.valid_until OR cv.revision IS NULL OR pv.revision IS NULL OR NOT ((cv.permissions_json||cv.grantable_json)<@pv.grantable_json)); $$;


CREATE FUNCTION people_case_active(case_key TEXT,at_time BIGINT) RETURNS BOOLEAN LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT 1 FROM people_lifecycle_cases c WHERE c.id=case_key AND c.status IN ('scheduled','completed') AND c.kind IN ('onboarding','transfer') AND c.effective_at<=at_time
 AND EXISTS(SELECT 1 FROM hr_engagements e JOIN hr_people h ON h.id=e.person_id AND h.company_id=e.company_id JOIN organization_positions p ON p.company_id=e.company_id AND p.id=c.definition->>'positionId' WHERE e.id=c.engagement_id AND e.status!='ended' AND h.status='active' AND h.merged_into_id IS NULL AND p.active=1 AND p.level_id=c.review_snapshot->>'levelId' AND p.branch_id IS NOT DISTINCT FROM c.review_snapshot->>'positionBranch' AND p.department_id IS NOT DISTINCT FROM c.review_snapshot->>'positionDepartment' AND (c.status='completed' AND e.position_id=p.id OR c.status='scheduled' AND e.version=(c.review_snapshot->>'engagementVersion')::integer))
 AND EXISTS(SELECT 1 FROM assignments a WHERE a.id=c.authorizing_assignment_id AND a.revoked_at IS NULL AND a.valid_from<=at_time AND a.valid_until>at_time AND iam_principal_active(a.user_id,a.company_id,at_time))
 AND NOT EXISTS(SELECT 1 FROM people_lifecycle_cases newer WHERE newer.company_id=c.company_id AND newer.person_id=c.person_id AND newer.id<>c.id AND newer.kind IN ('transfer','offboarding','suspend') AND newer.status IN ('scheduled','completed') AND newer.effective_at>=c.effective_at AND newer.effective_at<=at_time)); $$;

CREATE OR REPLACE FUNCTION um_group_member(company_key TEXT,person_key TEXT,group_key TEXT) RETURNS BOOLEAN LANGUAGE sql STABLE AS $$
 WITH RECURSIVE children AS (
 SELECT g.id,g.definition,ARRAY[g.id] visited FROM um_groups g WHERE g.company_id=company_key AND g.id=group_key AND NOT (g.definition->>'archived')::boolean AND um_person_in_unit(company_key,person_key,g.definition->>'scopeNodeId')
 UNION ALL SELECT g.id,g.definition,c.visited||g.id FROM um_groups g JOIN children c ON g.definition->>'parentId'=c.id WHERE g.company_id=company_key AND NOT (g.definition->>'archived')::boolean AND NOT g.id=ANY(c.visited) AND cardinality(c.visited)<32 AND um_person_in_unit(company_key,person_key,g.definition->>'scopeNodeId'))
 SELECT EXISTS(SELECT 1 FROM children c WHERE CASE WHEN c.definition->>'kind'='automatic' THEN um_rule_matches(c.definition->'rule',um_person_facts(company_key,person_key)) ELSE EXISTS(SELECT 1 FROM um_memberships m WHERE m.company_id=company_key AND m.group_id=c.id AND m.person_id=person_key) END OR EXISTS(SELECT 1 FROM people_lifecycle_cases lc JOIN people_package_revisions pr ON pr.company_id=lc.company_id AND pr.package_id=lc.package_id AND pr.revision=lc.package_revision WHERE lc.company_id=company_key AND lc.person_id=person_key AND people_case_active(lc.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND pr.definition->'groupIds' @> jsonb_build_array(c.id))); $$;

CREATE OR REPLACE FUNCTION people_lifecycle_grant_active(grant_key TEXT,at_time BIGINT) RETURNS BOOLEAN LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT 1 FROM people_lifecycle_grants g JOIN people_lifecycle_cases c ON c.id=g.case_id WHERE g.id=grant_key AND people_case_active(c.id,at_time) AND people_person_user(c.company_id,c.person_id,g.user_id)); $$;

CREATE FUNCTION people_protect_scheduled_administrator() RETURNS trigger LANGUAGE plpgsql AS $$
 BEGIN
 IF NEW.role='holding_owner' AND NEW.revoked_at IS NULL AND EXISTS(SELECT 1 FROM people_access_cutoffs x WHERE x.company_id=NEW.company_id AND x.cancelled_at IS NULL AND x.restored_at IS NULL AND x.effective_at<NEW.valid_until AND people_person_user(x.company_id,x.person_id,NEW.user_id)) THEN
 RAISE EXCEPTION 'Cancel the scheduled departure before assigning protected administration' USING ERRCODE='23514';
 END IF; RETURN NEW;
 END; $$;
CREATE TRIGGER people_protect_scheduled_administrator BEFORE INSERT OR UPDATE OF role,valid_until,revoked_at ON assignments FOR EACH ROW EXECUTE FUNCTION people_protect_scheduled_administrator();
CREATE FUNCTION people_protect_scheduled_identity() RETURNS trigger LANGUAGE plpgsql AS $$
 BEGIN
 IF (NEW.login_policy='emergency' OR EXISTS(SELECT 1 FROM assignments a WHERE a.company_id=NEW.company_id AND a.user_id=NEW.user_id AND a.role='holding_owner' AND a.revoked_at IS NULL)) AND EXISTS(SELECT 1 FROM people_access_cutoffs x WHERE x.company_id=NEW.company_id AND x.person_id=NEW.person_id AND x.cancelled_at IS NULL AND x.restored_at IS NULL) THEN
 RAISE EXCEPTION 'A protected administrator cannot be linked to a departing person' USING ERRCODE='23514';
 END IF; RETURN NEW;
 END; $$;
CREATE TRIGGER people_protect_scheduled_identity BEFORE INSERT OR UPDATE OF person_id,login_policy ON iam_members FOR EACH ROW EXECUTE FUNCTION people_protect_scheduled_identity();
