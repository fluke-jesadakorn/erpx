-- A personal plan orders references to existing work. It does not change task
-- deadlines, roster assignments, attendance, or the source workflow.
CREATE TABLE staff_daily_plans (
  company_id TEXT NOT NULL,
  user_id TEXT NOT NULL,
  plan_date TEXT NOT NULL CHECK (plan_date ~ '^[0-9]{4}-[0-9]{2}-[0-9]{2}$'),
  items_json TEXT NOT NULL DEFAULT '[]' CHECK (erpx_json_valid(items_json)),
  version BIGINT NOT NULL CHECK (version > 0),
  last_command_id TEXT NOT NULL,
  updated_at BIGINT NOT NULL,
  PRIMARY KEY (company_id, user_id, plan_date)
);

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
  EXECUTE format('GRANT SELECT,INSERT,UPDATE ON TABLE staff_daily_plans TO %I,%I', app_role, jobs_role);
END $$;
