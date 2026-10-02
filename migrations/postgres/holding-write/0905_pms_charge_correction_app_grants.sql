-- The PMS correction tables were introduced before the app role received
-- privileges. Front workspaces need to read the queue during initial load.
DO $$
DECLARE migration_role TEXT := current_user;
DECLARE app_role TEXT;
BEGIN
  IF migration_role !~ '_migration$' THEN RETURN; END IF;
  app_role := regexp_replace(migration_role, '_migration$', '_app');
  EXECUTE format('GRANT SELECT, INSERT, UPDATE ON pms_charge_corrections TO %I', app_role);
  EXECUTE format('GRANT SELECT, INSERT ON pms_charge_correction_events TO %I', app_role);
END $$;
