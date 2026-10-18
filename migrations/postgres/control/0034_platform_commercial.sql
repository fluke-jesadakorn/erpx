-- Explicit provider permissions. A Company purchase never creates Platform authority.
INSERT INTO iam_permissions(id,definition) VALUES
('platform.commercial.read','{"id":"platform.commercial.read","label":"Review Company package grants","module":"commercial","roles":["platform_admin","platform_auditor"],"conditions":["Current Platform authority"],"risk":"normal","availability":"live","namespace":"platform","permittedScopes":["platform"]}'),
('platform.commercial.manage','{"id":"platform.commercial.manage","label":"Apply reviewed Company package grants","module":"commercial","roles":["platform_admin"],"conditions":["Current Platform authority, reviewed preview and fresh MFA"],"risk":"privileged","availability":"live","namespace":"platform","permittedScopes":["platform"]}')
ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;

DO $$
DECLARE role_key text; prior record; next_revision integer; added jsonb; grantable_added jsonb;
BEGIN
  FOREACH role_key IN ARRAY ARRAY['platform_admin','platform_auditor'] LOOP
    SELECT r.published_revision,v.permissions_json,v.grantable_json,v.scopes_json INTO prior
      FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision
      WHERE r.company_id='*' AND r.id=role_key AND r.built_in AND r.status='active' FOR UPDATE OF r;
    IF NOT FOUND OR NOT prior.scopes_json @> '["platform"]'::jsonb THEN CONTINUE; END IF;
    added='[]'::jsonb; grantable_added='[]'::jsonb;
    -- Preserve deliberately restricted built-in ceilings; custom roles/assignments are untouched.
    IF prior.permissions_json @> '["platform.iam.read"]'::jsonb AND NOT prior.permissions_json @> '["platform.commercial.read"]'::jsonb THEN
      added=added||'["platform.commercial.read"]'::jsonb;
      IF prior.grantable_json @> '["platform.iam.read"]'::jsonb THEN grantable_added=grantable_added||'["platform.commercial.read"]'::jsonb; END IF;
    END IF;
    IF role_key='platform_admin' AND prior.permissions_json @> '["platform.iam.manage"]'::jsonb AND NOT prior.permissions_json @> '["platform.commercial.manage"]'::jsonb THEN
      added=added||'["platform.commercial.manage"]'::jsonb;
      IF prior.grantable_json @> '["platform.iam.manage"]'::jsonb THEN grantable_added=grantable_added||'["platform.commercial.manage"]'::jsonb; END IF;
    END IF;
    IF added='[]'::jsonb THEN CONTINUE; END IF;
    SELECT COALESCE(MAX(revision),0)+1 INTO next_revision FROM iam_role_revisions WHERE company_id='*' AND role_id=role_key;
    INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)
      VALUES('*',role_key,next_revision,prior.permissions_json||added,prior.grantable_json||grantable_added,prior.scopes_json,'system',floor(extract(epoch FROM clock_timestamp())*1000)::bigint);
    UPDATE iam_roles SET published_revision=next_revision WHERE company_id='*' AND id=role_key;
  END LOOP;
END $$;
