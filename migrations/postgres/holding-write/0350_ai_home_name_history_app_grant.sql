-- Mention validation reads prior assistant names, and AI settings retain a
-- previous name when the holding owner renames the assistant.
DO $$
DECLARE migration_role TEXT := current_user; app_role TEXT;
BEGIN
  IF migration_role !~ '_migration$' THEN RETURN; END IF;
  app_role := regexp_replace(migration_role, '_migration$', '_app');
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname=app_role) THEN
    RAISE EXCEPTION 'ERPX_APP_ROLE_MISSING';
  END IF;
  EXECUTE format('GRANT SELECT,INSERT ON TABLE ai_home_name_history TO %I', app_role);
END $$;
