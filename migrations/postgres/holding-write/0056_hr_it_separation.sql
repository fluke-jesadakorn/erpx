-- HR placement and IT application have independent clocks. Existing IDs/reviews survive.
ALTER TABLE people_lifecycle_cases ADD COLUMN workflow TEXT NOT NULL DEFAULT 'legacy' CHECK(workflow IN ('legacy','placement'));
ALTER TABLE people_directory_operations ADD COLUMN approval_mode TEXT NOT NULL DEFAULT 'reviewed' CHECK(approval_mode IN ('direct','reviewed'));
ALTER TABLE people_directory_operations ADD COLUMN mfa_session_id TEXT;
ALTER TABLE people_directory_operations ADD COLUMN steps JSONB NOT NULL DEFAULT '[]';

CREATE TABLE people_provision_operations (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, definition JSONB NOT NULL, evidence JSONB NOT NULL,
 status TEXT NOT NULL CHECK(status IN ('pending_review','queued','running','retry','completed','blocked','rejected')),
 version INTEGER NOT NULL DEFAULT 1, created_by TEXT NOT NULL, assignment_id TEXT NOT NULL REFERENCES assignments(id),
 session_id TEXT NOT NULL, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL,
 last_error TEXT, steps JSONB NOT NULL DEFAULT '[]', next_attempt_at BIGINT NOT NULL DEFAULT 0
);
CREATE TABLE people_applied_packages (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, person_id TEXT NOT NULL, engagement_id TEXT NOT NULL REFERENCES hr_engagements(id),
 position_id TEXT, package_id TEXT NOT NULL, package_revision INTEGER NOT NULL, definition JSONB NOT NULL,
 source_kind TEXT NOT NULL CHECK(source_kind IN ('position','supplemental')), scope JSONB NOT NULL,
 case_id TEXT UNIQUE REFERENCES people_lifecycle_cases(id), operation_id TEXT REFERENCES people_provision_operations(id),
 authorizing_assignment_id TEXT NOT NULL REFERENCES assignments(id), valid_from BIGINT NOT NULL, valid_until BIGINT NOT NULL,
 ended_at BIGINT, created_at BIGINT NOT NULL,
 FOREIGN KEY(company_id,package_id,package_revision) REFERENCES people_package_revisions(company_id,package_id,revision)
);
CREATE INDEX people_applied_person ON people_applied_packages(company_id,person_id) WHERE ended_at IS NULL;
CREATE TABLE people_provision_grants (
 id TEXT PRIMARY KEY, operation_id TEXT NOT NULL REFERENCES people_provision_operations(id), application_id TEXT NOT NULL REFERENCES people_applied_packages(id),
 definition JSONB NOT NULL, review_id TEXT NOT NULL REFERENCES iam_reviews(id), assignment_id TEXT REFERENCES assignments(id)
);
-- Snapshot only access effective before migration; never reactivate historical rights.
INSERT INTO people_applied_packages(id,company_id,person_id,engagement_id,position_id,package_id,package_revision,definition,source_kind,scope,case_id,authorizing_assignment_id,valid_from,valid_until,created_at)
 SELECT 'legacy:'||c.id,c.company_id,c.person_id,c.engagement_id,c.definition->>'positionId',c.package_id,c.package_revision,r.definition,'position',c.review_snapshot,c.id,c.authorizing_assignment_id,c.effective_at,a.valid_until,c.updated_at
 FROM people_lifecycle_cases c JOIN people_package_revisions r ON r.company_id=c.company_id AND r.package_id=c.package_id AND r.revision=c.package_revision JOIN assignments a ON a.id=c.authorizing_assignment_id
 WHERE c.status='completed' AND people_case_active(c.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint);
ALTER FUNCTION people_case_active(TEXT,BIGINT) RENAME TO people_legacy_case_active;
CREATE FUNCTION people_application_active(application_key TEXT,at_time BIGINT) RETURNS BOOLEAN LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT 1 FROM people_applied_packages ap JOIN hr_people p ON p.company_id=ap.company_id AND p.id=ap.person_id JOIN hr_engagements e ON e.id=ap.engagement_id AND e.company_id=ap.company_id
 JOIN assignments a ON a.id=ap.authorizing_assignment_id
 WHERE ap.id=application_key AND ap.ended_at IS NULL AND ap.valid_from<=at_time AND ap.valid_until>at_time
 AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active'
 AND a.revoked_at IS NULL AND a.valid_from<=at_time AND a.valid_until>at_time AND iam_principal_active(a.user_id,a.company_id,at_time)
 AND (ap.operation_id IS NULL OR EXISTS(SELECT 1 FROM people_provision_operations op WHERE op.id=ap.operation_id AND op.status='completed'))
 AND NOT EXISTS(SELECT 1 FROM people_access_cutoffs x WHERE x.company_id=ap.company_id AND x.person_id=ap.person_id AND x.cancelled_at IS NULL AND x.effective_at<=at_time AND (x.restored_at IS NULL OR x.effective_at>=ap.valid_from)));
$$;
CREATE FUNCTION people_case_active(case_key TEXT,at_time BIGINT) RETURNS BOOLEAN LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT 1 FROM people_applied_packages ap WHERE ap.case_id=case_key AND people_application_active(ap.id,at_time))
 OR EXISTS(SELECT 1 FROM people_lifecycle_cases c WHERE c.id=case_key AND c.workflow='legacy' AND c.status='scheduled' AND people_legacy_case_active(c.id,at_time));
$$;
CREATE OR REPLACE FUNCTION people_lifecycle_grant_active(grant_key TEXT,at_time BIGINT) RETURNS BOOLEAN LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT 1 FROM people_lifecycle_grants g JOIN people_lifecycle_cases c ON c.id=g.case_id WHERE g.id=grant_key AND people_case_active(c.id,at_time) AND people_person_user(c.company_id,c.person_id,g.user_id));
$$;
CREATE FUNCTION people_provision_grant_active(grant_key TEXT,at_time BIGINT) RETURNS BOOLEAN LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT 1 FROM people_provision_grants g JOIN people_applied_packages ap ON ap.id=g.application_id WHERE g.id=grant_key AND people_application_active(ap.id,at_time) AND people_person_user(ap.company_id,ap.person_id,g.definition->>'userId'));
$$;
ALTER TABLE assignments DROP CONSTRAINT assignments_source_check;
ALTER TABLE assignments ADD CONSTRAINT assignments_source_check CHECK(source IN ('legacy','manual','directory','emergency','policy','lifecycle','provisioning'));
-- Preserve the existing chain, expiry, principal and delegation checks verbatim.
DO $$ DECLARE body TEXT; BEGIN
 body := pg_get_functiondef('iam_assignment_active(text,bigint)'::regprocedure);
 body := replace(body,'source=''lifecycle'' AND NOT people_lifecycle_grant_active(source_id,at_time)', 'source=''lifecycle'' AND NOT people_lifecycle_grant_active(source_id,at_time) OR source=''provisioning'' AND NOT people_provision_grant_active(source_id,at_time)');
 EXECUTE body;
END $$;
CREATE OR REPLACE FUNCTION um_group_member(company_key TEXT,person_key TEXT,group_key TEXT) RETURNS BOOLEAN LANGUAGE sql STABLE AS $$
 WITH RECURSIVE children AS (
 SELECT g.id,g.definition,ARRAY[g.id] visited FROM um_groups g WHERE g.company_id=company_key AND g.id=group_key AND NOT (g.definition->>'archived')::boolean AND um_person_in_unit(company_key,person_key,g.definition->>'scopeNodeId')
 UNION ALL SELECT g.id,g.definition,c.visited||g.id FROM um_groups g JOIN children c ON g.definition->>'parentId'=c.id WHERE g.company_id=company_key AND NOT (g.definition->>'archived')::boolean AND NOT g.id=ANY(c.visited) AND cardinality(c.visited)<32 AND um_person_in_unit(company_key,person_key,g.definition->>'scopeNodeId'))
 SELECT EXISTS(SELECT 1 FROM children c WHERE CASE WHEN c.definition->>'kind'='automatic' THEN um_rule_matches(c.definition->'rule',um_person_facts(company_key,person_key)) ELSE EXISTS(SELECT 1 FROM um_memberships m WHERE m.company_id=company_key AND m.group_id=c.id AND m.person_id=person_key) END
 OR EXISTS(SELECT 1 FROM people_applied_packages ap WHERE ap.company_id=company_key AND ap.person_id=person_key AND people_application_active(ap.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND ap.definition->'groupIds' @> jsonb_build_array(c.id))
 OR EXISTS(SELECT 1 FROM people_lifecycle_cases lc JOIN people_package_revisions pr ON pr.company_id=lc.company_id AND pr.package_id=lc.package_id AND pr.revision=lc.package_revision WHERE lc.company_id=company_key AND lc.person_id=person_key AND lc.status='scheduled' AND lc.workflow='legacy' AND people_case_active(lc.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND pr.definition->'groupIds' @> jsonb_build_array(c.id)));
$$;
INSERT INTO iam_permissions VALUES('iam.directory.apply','{"id":"iam.directory.apply","label":"Apply directory and resource changes","module":"identity_access","roles":["holding_owner","it_admin"],"conditions":["Preview and fresh MFA"],"risk":"privileged","availability":"live","namespace":"customer","permittedScopes":["holding"]}');
INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)
 SELECT r.company_id,r.id,r.published_revision+1,v.permissions_json||'["iam.directory.apply"]'::jsonb,
 v.grantable_json||CASE WHEN r.id='holding_owner' THEN '["iam.directory.apply"]'::jsonb ELSE '[]'::jsonb END,
 v.scopes_json,'system:hr-it',floor(extract(epoch FROM clock_timestamp())*1000)::bigint
 FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.company_id='*' AND r.id IN ('holding_owner','it_admin');
UPDATE iam_roles r SET published_revision=(SELECT max(revision) FROM iam_role_revisions v WHERE v.company_id=r.company_id AND v.role_id=r.id) WHERE r.company_id='*' AND r.id IN ('holding_owner','it_admin');
ALTER TABLE people_directory_operations ADD COLUMN provision_id TEXT REFERENCES people_provision_operations(id);
ALTER TABLE people_preparation_tasks ADD COLUMN application_id TEXT REFERENCES people_applied_packages(id);
CREATE TABLE people_directory_membership_sources (
 connection_id TEXT NOT NULL REFERENCES iam_connections(id), group_id TEXT NOT NULL, member_id TEXT NOT NULL,
 source_id TEXT NOT NULL, application_id TEXT REFERENCES people_applied_packages(id), created_at BIGINT NOT NULL,
 PRIMARY KEY(connection_id,group_id,member_id,source_id)
);
