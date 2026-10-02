-- Background AI workers reserve and finish gateway requests themselves.
-- Keep policy and cost-evidence administration read-only for the jobs role.
DO $$
DECLARE migration_role TEXT := current_user; jobs_role TEXT;
BEGIN
  IF migration_role !~ '_migration$' THEN RETURN; END IF;
  jobs_role := regexp_replace(migration_role, '_migration$', '_jobs');
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname=jobs_role) THEN
    RAISE EXCEPTION 'ERPX_JOBS_ROLE_MISSING';
  END IF;
  EXECUTE format('GRANT SELECT,INSERT,UPDATE ON TABLE ai_gateway_requests TO %I',jobs_role);
END $$;
