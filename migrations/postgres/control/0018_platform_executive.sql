INSERT INTO iam_permissions(id,definition)
VALUES(
  'platform.executive.read',
  '{"id":"platform.executive.read","label":"View Platform Executive insights","module":"platform_administration","roles":["platform_admin","platform_auditor"],"conditions":["PlatformContext and control-plane data only"],"risk":"normal","availability":"live","namespace":"platform","permittedScopes":["platform"]}'::jsonb
)
ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;

DO $$
DECLARE
  role_key text;
  prior_revision integer;
  next_revision integer;
  prior_permissions jsonb;
  prior_grantable jsonb;
BEGIN
  FOREACH role_key IN ARRAY ARRAY['platform_admin','platform_auditor'] LOOP
    SELECT r.published_revision,v.permissions_json,v.grantable_json
      INTO prior_revision,prior_permissions,prior_grantable
      FROM iam_roles r
      JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision
      WHERE r.company_id='*' AND r.id=role_key AND r.status='active'
      FOR UPDATE OF r;
    IF NOT FOUND THEN CONTINUE; END IF;
    IF prior_permissions @> '["platform.executive.read"]'::jsonb THEN CONTINUE; END IF;
    SELECT COALESCE(MAX(revision),0)+1 INTO next_revision
      FROM iam_role_revisions WHERE company_id='*' AND role_id=role_key;
    INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)
      VALUES(
        '*',role_key,next_revision,
        prior_permissions || '["platform.executive.read"]'::jsonb,
        CASE WHEN role_key='platform_admin' THEN prior_grantable || '["platform.executive.read"]'::jsonb ELSE prior_grantable END,
        '["platform"]'::jsonb,'system',floor(extract(epoch FROM clock_timestamp())*1000)::bigint
      );
    UPDATE iam_roles SET published_revision=next_revision WHERE company_id='*' AND id=role_key;
  END LOOP;
END $$;
