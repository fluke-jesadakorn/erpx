-- Provisioning intentionally grants broad defaults to the application and
-- jobs logins. Narrow the three operational-only target billing tables after
-- creation, so a future application query cannot create a price, designate a
-- test Holding, or forge principal-status history outside reviewed commands.
ALTER FUNCTION public.capture_billing_principal_status() SECURITY DEFINER;
ALTER FUNCTION public.capture_billing_principal_status()
  SET search_path = pg_catalog, public;

DO $$
DECLARE
  runtime_role TEXT;
  role_name TEXT;
  prefix TEXT;
BEGIN
  IF right(current_user,10) <> '_migration' THEN
    RETURN;
  END IF;
  prefix := left(current_user,length(current_user)-10);
  FOREACH runtime_role IN ARRAY ARRAY['app','jobs'] LOOP
    role_name := prefix || '_' || runtime_role;
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname=role_name) THEN
      EXECUTE format('GRANT SELECT ON TABLE billing_target_prices,billing_target_sandbox_holdings,billing_principal_status_events,billing_principal_status_coverage TO %I',role_name);
      EXECUTE format('REVOKE INSERT,UPDATE,DELETE ON TABLE billing_target_prices,billing_target_sandbox_holdings,billing_principal_status_events,billing_principal_status_coverage FROM %I',role_name);
      EXECUTE format('REVOKE ALL ON SEQUENCE billing_principal_status_events_id_seq FROM %I',role_name);
    END IF;
  END LOOP;
END;
$$;
