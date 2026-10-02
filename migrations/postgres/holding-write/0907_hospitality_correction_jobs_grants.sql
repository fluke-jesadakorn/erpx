DO $$
DECLARE jobs_role TEXT;
BEGIN
 IF current_user !~ '_migration$' THEN RETURN; END IF;
 jobs_role := regexp_replace(current_user, '_migration$', '_jobs');
 EXECUTE format('GRANT SELECT, INSERT, UPDATE ON pms_charge_corrections TO %I',jobs_role);
 EXECUTE format('GRANT SELECT, INSERT ON pms_charge_correction_events TO %I',jobs_role);
END $$;
