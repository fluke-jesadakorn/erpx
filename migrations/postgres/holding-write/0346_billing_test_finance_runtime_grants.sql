-- Billing jobs post to the isolated test ledger using the jobs role. Keep the
-- app role and customer Finance screens unable to write these records.
DO $$
DECLARE migration_role TEXT := current_user; jobs_role TEXT;
BEGIN
  IF migration_role !~ '_migration$' THEN RETURN; END IF;
  jobs_role := regexp_replace(migration_role, '_migration$', '_jobs');
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname=jobs_role) THEN
    RAISE EXCEPTION 'ERPX_JOBS_ROLE_MISSING';
  END IF;
  EXECUTE format('GRANT SELECT,INSERT,UPDATE ON TABLE billing_test_finance_documents TO %I',jobs_role);
  EXECUTE format('GRANT SELECT,INSERT ON TABLE billing_test_finance_entries,billing_test_finance_lines TO %I',jobs_role);
END $$;
