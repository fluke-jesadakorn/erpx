-- Relationship authority is independent of account/role administration.
INSERT INTO iam_permissions(id,definition) VALUES
 ('people.relationships.read','{"id":"people.relationships.read","label":"Read people relationships","module":"people_payroll","roles":["holding_owner","hr_admin","it_admin"],"conditions":["Company scope"],"risk":"normal","availability":"live","namespace":"customer","permittedScopes":["holding"]}'),
 ('people.relationships.manage','{"id":"people.relationships.manage","label":"Maintain people relationships","module":"people_payroll","roles":["holding_owner","hr_admin","it_admin"],"conditions":["Company scope; relationship metadata only"],"risk":"privileged","availability":"live","namespace":"customer","permittedScopes":["holding"]}');
DO $$ DECLARE r record; rev integer; perms jsonb; ceiling jsonb; BEGIN
 FOR r IN SELECT roles.*,v.permissions_json,v.grantable_json,v.scopes_json FROM iam_roles roles
 JOIN iam_role_revisions v ON v.company_id=roles.company_id AND v.role_id=roles.id AND v.revision=roles.published_revision
 WHERE roles.company_id='*' AND roles.id IN ('hr_admin','it_admin','holding_owner') LOOP
  rev:=r.published_revision+1;
  SELECT jsonb_agg(DISTINCT p) INTO perms FROM jsonb_array_elements_text(r.permissions_json||'["people.relationships.read","people.relationships.manage"]') p;
  ceiling:=r.grantable_json;
  IF r.id IN ('holding_owner','it_admin') THEN
   SELECT jsonb_agg(DISTINCT p) INTO ceiling FROM jsonb_array_elements_text(ceiling||'["people.relationships.read","people.relationships.manage"]') p;
  END IF;
  INSERT INTO iam_role_revisions VALUES(r.company_id,r.id,rev,perms,ceiling,r.scopes_json,'system:people-relationships',floor(extract(epoch FROM clock_timestamp())*1000)::bigint);
  UPDATE iam_roles SET published_revision=rev WHERE company_id=r.company_id AND id=r.id;
 END LOOP;
END $$;
-- Classification is a read-model property. No account, invitation or grant is created.

-- Relationship edits cannot bypass an invitation's proof of identity, even if
-- HR sets the relationship active before the account owner accepts it.
CREATE OR REPLACE FUNCTION iam_principal_active(user_key text,company_key text,at_time bigint) RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT iam_principal_active_before_external(user_key,company_key,at_time)
 AND NOT EXISTS(SELECT 1 FROM iam_access_subjects s WHERE s.company_id=company_key
   AND (s.user_id=user_key OR s.person_id IS NOT NULL AND EXISTS(SELECT 1 FROM iam_members m WHERE m.company_id=company_key AND m.user_id=user_key AND m.person_id=s.person_id))
   AND (s.status!='active' OR s.kind='external' AND s.expires_at IS NOT NULL AND s.expires_at<=at_time
     OR EXISTS(SELECT 1 FROM iam_access_invitations i WHERE i.company_id=s.company_id AND i.subject_id=s.id AND i.state='authorized')));
$$;
