-- A scheduled draft is created by the system. Human review belongs to the
-- preparer who submits the calculated revision, not to the draft creator.
ALTER TABLE hr_payroll_runs ADD COLUMN submitted_by TEXT;
ALTER TABLE hr_payroll_runs ADD COLUMN submitted_digest TEXT;
ALTER TABLE hr_payroll_runs ADD COLUMN submitted_at BIGINT;
ALTER TABLE hr_payroll_runs DROP CONSTRAINT hr_payroll_runs_status_check;
ALTER TABLE hr_payroll_runs ADD CONSTRAINT hr_payroll_runs_status_check
  CHECK (status IN ('draft','calculating','preview','in_review','approved','posted','cancelled','failed'));
ALTER TABLE hr_payroll_runs ADD CONSTRAINT hr_payroll_runs_submission_complete_check
  CHECK ((submitted_by IS NULL AND submitted_digest IS NULL AND submitted_at IS NULL)
      OR (submitted_by IS NOT NULL AND submitted_digest IS NOT NULL AND submitted_at IS NOT NULL));

ALTER TABLE hr_pay_groups ADD COLUMN schedule_enabled BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE hr_pay_groups ADD COLUMN pay_delay_days INTEGER NOT NULL DEFAULT 1
  CHECK (pay_delay_days BETWEEN 1 AND 45);
ALTER TABLE hr_pay_groups ADD COLUMN schedule_anchor_date TEXT;
ALTER TABLE hr_pay_groups ADD COLUMN schedule_version BIGINT NOT NULL DEFAULT 1
  CHECK (schedule_version > 0);
ALTER TABLE hr_pay_groups ADD CONSTRAINT hr_pay_groups_schedule_anchor_check
  CHECK (schedule_anchor_date IS NULL OR schedule_anchor_date ~ '^[0-9]{4}-[0-9]{2}-[0-9]{2}$');
