-- Appointment and published flow history is append only. Corrections require a new version.
CREATE FUNCTION department_authority_history_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 RAISE EXCEPTION 'DEPARTMENT_AUTHORITY_HISTORY_IMMUTABLE' USING ERRCODE='23514';
END $$;
CREATE TRIGGER authority_appointments_immutable BEFORE UPDATE OR DELETE ON authority_appointments
 FOR EACH ROW EXECUTE FUNCTION department_authority_history_immutable();
CREATE TRIGGER department_flow_versions_immutable BEFORE UPDATE OR DELETE ON department_flow_versions
 FOR EACH ROW EXECUTE FUNCTION department_authority_history_immutable();
CREATE TRIGGER position_gpo_revisions_immutable BEFORE UPDATE OR DELETE ON position_gpo_revisions
 FOR EACH ROW EXECUTE FUNCTION department_authority_history_immutable();
