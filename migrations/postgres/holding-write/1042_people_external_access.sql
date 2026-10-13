-- External organizations are IAM metadata, never organization_nodes/Companies.
CREATE TABLE iam_external_organizations (
 id text PRIMARY KEY,company_id text NOT NULL,name text NOT NULL,
 kind text NOT NULL CHECK(kind IN ('organization','independent')),version integer NOT NULL DEFAULT 1,
 UNIQUE(company_id,id)
);
CREATE TABLE iam_external_teams (
 id text PRIMARY KEY,company_id text NOT NULL,organization_id text NOT NULL,name text NOT NULL,
 department_id text,version integer NOT NULL DEFAULT 1,UNIQUE(company_id,id),
 FOREIGN KEY(company_id,organization_id) REFERENCES iam_external_organizations(company_id,id)
);
CREATE TABLE iam_access_subjects (
 id text PRIMARY KEY,company_id text NOT NULL,kind text NOT NULL CHECK(kind IN ('internal','external')),
 name text NOT NULL,email text NOT NULL,user_id text,person_id text,organization_id text,team_id text,
 sponsor_person_id text,expires_at bigint,status text NOT NULL DEFAULT 'active' CHECK(status IN ('active','invited','suspended')),
 version integer NOT NULL DEFAULT 1,UNIQUE(company_id,user_id),UNIQUE(company_id,id),
 FOREIGN KEY(company_id,organization_id) REFERENCES iam_external_organizations(company_id,id),
 FOREIGN KEY(company_id,team_id) REFERENCES iam_external_teams(company_id,id)
);
CREATE INDEX iam_access_subject_person ON iam_access_subjects(company_id,person_id);
CREATE TABLE iam_access_previews (
 token text PRIMARY KEY,company_id text NOT NULL,actor_id text NOT NULL,assignment_id text NOT NULL,
 session_id text NOT NULL,change jsonb NOT NULL,fingerprint text NOT NULL,expires_at bigint NOT NULL
);
CREATE TABLE iam_access_invitations (
 id text PRIMARY KEY,company_id text NOT NULL,subject_id text NOT NULL,definition jsonb NOT NULL,
 author_id text NOT NULL,parent_id text NOT NULL,role_revision integer NOT NULL,
 state text NOT NULL DEFAULT 'authorized' CHECK(state IN ('authorized','accepted','revoked')),
 FOREIGN KEY(company_id,subject_id) REFERENCES iam_access_subjects(company_id,id)
);

-- Retain all contractor business history. Missing sponsorship/expiry is intentionally
-- incomplete metadata; legacy grants keep working until IT explicitly manages it.
INSERT INTO iam_external_organizations(id,company_id,name,kind)
 SELECT DISTINCT 'external-legacy-'||e.company_id,e.company_id,'Existing contractors','independent'
 FROM hr_engagements e WHERE e.worker_type='contractor';
INSERT INTO iam_access_subjects(id,company_id,kind,name,email,user_id,person_id,organization_id)
 SELECT DISTINCT ON (p.company_id,p.id) 'external-legacy-'||p.id,p.company_id,'external',p.legal_name,
 COALESCE(p.work_email,p.email,''),COALESCE(p.user_id,(SELECT m.user_id FROM iam_members m WHERE m.company_id=p.company_id AND m.person_id=p.id AND m.status='active' ORDER BY m.user_id LIMIT 1)),p.id,'external-legacy-'||p.company_id
 FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id
 WHERE e.worker_type='contractor' AND NOT EXISTS(SELECT 1 FROM hr_engagements employee
  WHERE employee.person_id=p.id AND employee.company_id=p.company_id AND employee.worker_type='employee' AND employee.status IN ('active','onboarding','suspended'))
 ORDER BY p.company_id,p.id,e.start_date DESC;

-- All aliases of an explicitly managed external relationship share the cutoff.
-- Preserve existing cutoff, directory and lifecycle checks by composing their function.
ALTER FUNCTION iam_principal_active(text,text,bigint) RENAME TO iam_principal_active_before_external;
CREATE FUNCTION iam_principal_active(user_key text,company_key text,at_time bigint) RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT iam_principal_active_before_external(user_key,company_key,at_time)
 AND NOT EXISTS(SELECT 1 FROM iam_access_subjects s WHERE s.company_id=company_key
   AND (s.user_id=user_key OR s.person_id IS NOT NULL AND EXISTS(SELECT 1 FROM iam_members m WHERE m.company_id=company_key AND m.user_id=user_key AND m.person_id=s.person_id))
   AND (s.status!='active' OR s.kind='external' AND s.expires_at IS NOT NULL AND s.expires_at<=at_time));
$$;

-- Remove HR management permissions without shrinking its historical delegation
-- evidence, which would invalidate unrelated descendants. It cannot issue new grants.
DO $$ DECLARE r record;rev integer;perms jsonb;ceiling jsonb; BEGIN
 FOR r IN SELECT roles.*,v.permissions_json,v.grantable_json,v.scopes_json FROM iam_roles roles
  JOIN iam_role_revisions v ON v.company_id=roles.company_id AND v.role_id=roles.id AND v.revision=roles.published_revision
  WHERE roles.company_id='*' AND roles.id IN ('hr_admin','it_admin','holding_owner') LOOP
  rev:=r.published_revision+1;perms:=r.permissions_json;ceiling:=r.grantable_json;
  IF r.id='hr_admin' THEN SELECT COALESCE(jsonb_agg(p),'[]') INTO perms FROM jsonb_array_elements_text(perms) p WHERE p NOT LIKE 'iam.%' OR p='iam.self'; END IF;
  IF r.id IN ('it_admin','holding_owner') THEN
   SELECT jsonb_agg(DISTINCT p) INTO perms FROM jsonb_array_elements_text(perms||'["iam.manage","iam.roles.manage","iam.access.apply"]') p;
  END IF;
  IF r.id='it_admin' THEN
   SELECT COALESCE(jsonb_agg(p),'[]') INTO ceiling FROM iam_roles owner_role JOIN iam_role_revisions v ON v.company_id=owner_role.company_id AND v.role_id=owner_role.id AND v.revision=owner_role.published_revision,
    jsonb_array_elements_text(v.grantable_json) p
    WHERE owner_role.id='holding_owner' AND owner_role.company_id='*' AND p NOT IN ('company.owner','iam.recovery','iam.review') AND p NOT LIKE 'platform.%';
  END IF;
  IF r.id IN ('it_admin','holding_owner') THEN
   SELECT jsonb_agg(DISTINCT p) INTO ceiling FROM jsonb_array_elements_text(ceiling||'["iam.access.apply"]') p;
  END IF;
  INSERT INTO iam_role_revisions VALUES(r.company_id,r.id,rev,perms,ceiling,r.scopes_json,'system:people-it-separation',floor(extract(epoch FROM clock_timestamp())*1000)::bigint);
  UPDATE iam_roles SET published_revision=rev WHERE company_id=r.company_id AND id=r.id;
 END LOOP;
END $$;
INSERT INTO iam_permissions VALUES('iam.access.apply','{"id":"iam.access.apply","label":"Apply People identities and access","module":"identity_access","roles":["holding_owner","it_admin"],"conditions":["Preview and fresh MFA; delegated ceiling"],"risk":"privileged","availability":"live","namespace":"customer"}');
UPDATE iam_permissions SET definition=jsonb_set(definition,'{roles}','["holding_owner","it_admin","auditor"]') WHERE id='iam.read';
UPDATE iam_permissions SET definition=jsonb_set(definition,'{roles}','["holding_owner","it_admin"]') WHERE id IN ('iam.manage','iam.roles.manage');
