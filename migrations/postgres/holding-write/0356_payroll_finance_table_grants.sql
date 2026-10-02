-- These tables were added after the baseline app-role grants. Keep migration
-- ownership while allowing the application to read and write its workflows.
DO $$
DECLARE migration_role text := current_user;
DECLARE app_role text;
DECLARE jobs_role text;
BEGIN
  IF migration_role !~ '_migration$' THEN RETURN; END IF;
  app_role := regexp_replace(migration_role, '_migration$', '_app');
  jobs_role := regexp_replace(migration_role, '_migration$', '_jobs');
  EXECUTE format('GRANT SELECT, INSERT, UPDATE ON hr_payroll_account_mappings, hr_payroll_payment_settlements, hr_payroll_statutory_remittances TO %I', app_role);
  EXECUTE format('GRANT SELECT ON hr_payroll_account_mappings, hr_payroll_payment_settlements, hr_payroll_statutory_remittances TO %I', jobs_role);
END $$;
