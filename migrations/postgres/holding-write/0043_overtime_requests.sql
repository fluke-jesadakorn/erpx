ALTER TABLE approval_instances DROP CONSTRAINT approval_instances_record_type_check;
ALTER TABLE approval_instances ADD CONSTRAINT approval_instances_record_type_check CHECK(record_type IN ('expense','leave','leave_cancellation','task','overtime'));
CREATE TABLE overtime_eligibility (
 company_id TEXT NOT NULL, engagement_id TEXT NOT NULL REFERENCES hr_engagements(id), eligible BOOLEAN NOT NULL,
 reason TEXT NOT NULL, updated_by TEXT NOT NULL, updated_at BIGINT NOT NULL, PRIMARY KEY(company_id,engagement_id)
);
CREATE TABLE overtime_requests (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, engagement_id TEXT NOT NULL REFERENCES hr_engagements(id),
 person_id TEXT NOT NULL, branch_id TEXT, department_id TEXT, created_by TEXT NOT NULL,
 mode TEXT NOT NULL CHECK(mode IN ('advance','retrospective')), state TEXT NOT NULL DEFAULT 'draft'
 CHECK(state IN ('draft','pending_authorization','authorized','pending_verification','pending_hr','returned','rejected','withdrawn','approved')),
 timezone TEXT NOT NULL, start_local TEXT NOT NULL, end_local TEXT NOT NULL,
 starts_at BIGINT NOT NULL, ends_at BIGINT NOT NULL, break_minutes INTEGER NOT NULL,
 minutes INTEGER NOT NULL CHECK(minutes>0), reason TEXT NOT NULL, late_explanation TEXT NOT NULL DEFAULT '',
 roster_shift_id TEXT REFERENCES people_roster_shifts(id), planned_snapshot JSONB, actuals_snapshot JSONB,
 decision_note TEXT NOT NULL DEFAULT '', version INTEGER NOT NULL DEFAULT 1, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL,
 CHECK(ends_at>starts_at), CHECK(break_minutes>=0), UNIQUE(company_id,id)
);
CREATE INDEX overtime_personal ON overtime_requests(company_id,engagement_id,updated_at);
CREATE TABLE overtime_payroll_handoffs (
 id TEXT PRIMARY KEY, request_id TEXT NOT NULL UNIQUE REFERENCES overtime_requests(id), company_id TEXT NOT NULL,
 engagement_id TEXT NOT NULL REFERENCES hr_engagements(id), request_version INTEGER NOT NULL,
 work_date TEXT NOT NULL, minutes INTEGER NOT NULL CHECK(minutes>0), timezone TEXT NOT NULL,
 approved_by TEXT NOT NULL, created_at BIGINT NOT NULL
);
CREATE INDEX overtime_payroll_period ON overtime_payroll_handoffs(company_id,engagement_id,work_date);
CREATE FUNCTION overtime_handoff_immutable() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'Approved overtime hours are immutable'; END $$;
CREATE TRIGGER overtime_handoff_immutable BEFORE UPDATE OR DELETE ON overtime_payroll_handoffs FOR EACH ROW EXECUTE FUNCTION overtime_handoff_immutable();
-- Prevent manual time entry from reintroducing hours already handed off.
CREATE FUNCTION overtime_manual_guard() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF NEW.overtime_hours>0 AND NEW.status IN ('draft','submitted','approved') AND EXISTS(
  SELECT 1 FROM overtime_payroll_handoffs h JOIN overtime_requests r ON r.id=h.request_id
  WHERE h.company_id=NEW.company_id AND h.engagement_id=NEW.engagement_id
  AND NEW.work_date BETWEEN left(r.start_local,10) AND left(r.end_local,10)) THEN RAISE EXCEPTION 'OVERTIME_RECONCILIATION_REQUIRED'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER overtime_manual_guard BEFORE INSERT OR UPDATE ON hr_time_entries FOR EACH ROW EXECUTE FUNCTION overtime_manual_guard();
