-- Runtime roles do not inherit privileges on tables created by later migrations.
DO $$
DECLARE migration_role TEXT := current_user; app_role TEXT; jobs_role TEXT;
BEGIN
  IF migration_role !~ '_migration$' THEN RETURN; END IF;
  app_role := regexp_replace(migration_role, '_migration$', '_app');
  jobs_role := regexp_replace(migration_role, '_migration$', '_jobs');
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = app_role)
    OR NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = jobs_role) THEN
    RAISE EXCEPTION 'ERPX_RUNTIME_ROLES_MISSING';
  END IF;
  EXECUTE format('GRANT SELECT,INSERT,UPDATE,DELETE ON TABLE social_post_revisions,social_post_participants,social_post_sources TO %I,%I', app_role, jobs_role);
END $$;
