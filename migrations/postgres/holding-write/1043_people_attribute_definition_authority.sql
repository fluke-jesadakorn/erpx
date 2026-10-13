-- Role authoring must not allow IT to rewrite HR-owned custom field ACLs.
INSERT INTO iam_permissions VALUES('iam.attributes.manage','{"id":"iam.attributes.manage","label":"Manage custom field definitions and access","module":"identity_access","roles":["holding_owner"],"conditions":["Field ownership and access policy"],"risk":"privileged","availability":"live","namespace":"customer"}');
DO $$ DECLARE r record;perms jsonb;ceiling jsonb;cutoff bigint; BEGIN
 SELECT max(created_at) INTO cutoff FROM iam_role_revisions WHERE created_by='system:people-it-separation';
 FOR r IN SELECT roles.*,v.permissions_json,v.grantable_json,v.scopes_json,v.created_at FROM iam_roles roles
  JOIN iam_role_revisions v ON v.company_id=roles.company_id AND v.role_id=roles.id AND v.revision=roles.published_revision
  WHERE roles.id='holding_owner' AND roles.company_id='*'
   OR NOT roles.built_in AND v.created_at<=cutoff
    AND (v.permissions_json @> '["iam.roles.manage"]' OR v.grantable_json @> '["iam.roles.manage"]') LOOP
  perms:=r.permissions_json;ceiling:=r.grantable_json;
  IF perms @> '["iam.roles.manage"]' THEN
   SELECT jsonb_agg(DISTINCT p) INTO perms FROM jsonb_array_elements_text(perms||'["iam.attributes.manage"]') p;
  END IF;
  IF ceiling @> '["iam.roles.manage"]' THEN
   SELECT jsonb_agg(DISTINCT p) INTO ceiling FROM jsonb_array_elements_text(ceiling||'["iam.attributes.manage"]') p;
  END IF;
  INSERT INTO iam_role_revisions VALUES(r.company_id,r.id,r.published_revision+1,perms,ceiling,r.scopes_json,'system:attribute-authority-separation',floor(extract(epoch FROM clock_timestamp())*1000)::bigint);
  UPDATE iam_roles SET published_revision=r.published_revision+1 WHERE company_id=r.company_id AND id=r.id;
 END LOOP;
END $$;
