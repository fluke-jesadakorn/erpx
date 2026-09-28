-- Scanner jobs persist sanitized preview accounting after a source is clean.
-- Keep this narrower than the application grant: workers may append physical
-- storage rows, but the existing immutability triggers still prohibit edits.
DO $$
DECLARE migration_role TEXT := current_user; jobs_role TEXT;
BEGIN
  IF migration_role !~ '_migration$' THEN RETURN; END IF;
  jobs_role := regexp_replace(migration_role, '_migration$', '_jobs');
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname=jobs_role) THEN
    RAISE EXCEPTION 'ERPX_RUNTIME_ROLES_MISSING';
  END IF;
  EXECUTE format('GRANT INSERT ON TABLE attachment_storage_ledger TO %I', jobs_role);
END $$;
