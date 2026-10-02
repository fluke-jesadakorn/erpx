DO $$
DECLARE
  app_role TEXT := left(current_user, length(current_user) - 10) || '_app';
BEGIN
  IF right(current_user, 10) = '_migration'
     AND EXISTS (SELECT 1 FROM pg_roles WHERE rolname = app_role) THEN
    EXECUTE format('GRANT SELECT,INSERT,UPDATE ON member_profiles TO %I', app_role);
  END IF;
END;
$$;
