-- Direct development migrations need the same grants as the provisioner.
DO $$
DECLARE
  migration_role text := current_user;
  app_role text;
  jobs_role text;
BEGIN
  IF migration_role !~ '_migration$' THEN RETURN; END IF;
  app_role := regexp_replace(migration_role, '_migration$', '_app');
  jobs_role := regexp_replace(migration_role, '_migration$', '_jobs');
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = app_role)
     OR NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = jobs_role) THEN
    RAISE EXCEPTION 'ERPX_RUNTIME_ROLES_MISSING';
  END IF;
  EXECUTE format('GRANT SELECT,INSERT,UPDATE,DELETE ON TABLE position_pay_defaults,level_leave_defaults TO %I,%I',app_role,jobs_role);
END $$;
