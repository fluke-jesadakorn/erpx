-- Only normal Calendar commands capture actual queued authority. Historical
-- appointments/reminders are retained without a fabricated principal/backfill.
CREATE TABLE calendar_reminder_execution_authorizations (
  command_id TEXT NOT NULL REFERENCES command_receipts(command_id),
  appointment_id TEXT NOT NULL REFERENCES calendar_appointments(id),
  recipient_id TEXT NOT NULL,
  company_id TEXT NOT NULL,
  actor_id TEXT NOT NULL,
  assignment_id TEXT NOT NULL,
  action TEXT NOT NULL CHECK(action IN ('appointment.save','appointment.reschedule','appointment.reminder.set')),
  authority_json TEXT NOT NULL CHECK(erpx_json_valid(authority_json)),
  source_hash TEXT NOT NULL CHECK(length(source_hash)=64),
  source_version BIGINT NOT NULL CHECK(source_version>0),
  created_at BIGINT NOT NULL,
  PRIMARY KEY(command_id,appointment_id,recipient_id),
  CHECK(authority_json::jsonb->>'userId'=actor_id),
  CHECK(authority_json::jsonb->>'assignmentId'=assignment_id),
  CHECK(authority_json::jsonb->>'companyId'=company_id)
);
CREATE INDEX calendar_reminder_execution_source ON calendar_reminder_execution_authorizations(appointment_id,recipient_id,created_at DESC,command_id DESC);
CREATE FUNCTION calendar_reminder_execution_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION 'CALENDAR_REMINDER_AUTHORITY_IMMUTABLE' USING ERRCODE='23514';
END $$;
CREATE TRIGGER calendar_reminder_execution_immutable BEFORE UPDATE OR DELETE ON calendar_reminder_execution_authorizations
FOR EACH ROW EXECUTE FUNCTION calendar_reminder_execution_immutable();
