-- Match the pinned provisioning role convention without changing a migration
-- receipt that may already exist in a development Holding.
DO $$
DECLARE app_role TEXT:=regexp_replace(current_user,'_migration$','_app');
BEGIN
  IF right(current_user,10)='_migration' AND EXISTS(SELECT 1 FROM pg_roles WHERE rolname=app_role) THEN
    EXECUTE format('GRANT SELECT,INSERT,UPDATE ON finance_tax_entries TO %I',app_role);
    EXECUTE format('GRANT SELECT,INSERT ON finance_statutory_package_events TO %I',app_role);
  END IF;
END;
$$;
