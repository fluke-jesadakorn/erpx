-- The migration owner creates these tables, but the app and jobs use separate
-- least-privilege database roles. Seat events remain writable only by their
-- owner through the capture triggers, so callers cannot fabricate history.
ALTER FUNCTION billing_seat_assignment_capture() SECURITY DEFINER;
ALTER FUNCTION billing_seat_assignment_capture() SET search_path = pg_catalog, public;
ALTER FUNCTION billing_seat_member_capture() SECURITY DEFINER;
ALTER FUNCTION billing_seat_member_capture() SET search_path = pg_catalog, public;
ALTER FUNCTION billing_seat_role_capture() SECURITY DEFINER;
ALTER FUNCTION billing_seat_role_capture() SET search_path = pg_catalog, public;

DO $$
DECLARE migration_role TEXT := current_user; app_role TEXT; jobs_role TEXT;
BEGIN
  IF migration_role !~ '_migration$' THEN RETURN; END IF;
  app_role := regexp_replace(migration_role, '_migration$', '_app');
  jobs_role := regexp_replace(migration_role, '_migration$', '_jobs');
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname=app_role)
    OR NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname=jobs_role) THEN
    RAISE EXCEPTION 'ERPX_RUNTIME_ROLES_MISSING';
  END IF;
  EXECUTE format('GRANT SELECT,UPDATE ON TABLE ai_gateway_policies TO %I',app_role);
  EXECUTE format('GRANT SELECT,INSERT ON TABLE ai_gateway_policy_audit TO %I',app_role);
  EXECUTE format('GRANT SELECT,INSERT,UPDATE ON TABLE ai_gateway_requests,ai_provider_statements,ai_provider_cost_evidence TO %I',app_role);
  EXECUTE format('GRANT SELECT ON TABLE ai_gateway_policies,ai_gateway_requests,ai_provider_statements,ai_provider_cost_evidence TO %I',jobs_role);
  EXECUTE format('GRANT SELECT ON TABLE billing_seat_history_coverage,billing_seat_assignment_events,billing_seat_member_events,billing_seat_role_events TO %I,%I',app_role,jobs_role);
END $$;
