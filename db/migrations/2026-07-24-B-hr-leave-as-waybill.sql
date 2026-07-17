-- 2026-07-24-B-hr-leave-as-waybill.sql
-- HR leave requests become Waybills (origin='hr_leave'). Detail data lives
-- in the hr_leave table; the immutable event chain lives in waybill_events
-- and is signed with SESSION_SECRET.
--
-- Pipeline:
--   submission → hr_review → hr_authorization → disbursed
-- (3 stages, identical to the Waybill system used for expense/PR/PO.)

BEGIN;

-- 1. Allow origin='hr_leave' on waybills.
ALTER TABLE waybills DROP CONSTRAINT IF EXISTS waybills_origin_check;
ALTER TABLE waybills ADD CONSTRAINT waybills_origin_check
  CHECK (origin IN ('expense','pr','po','so','hr_leave'));

ALTER TABLE waybills DROP CONSTRAINT IF EXISTS waybills_waybill_kind_check;
ALTER TABLE waybills ADD CONSTRAINT waybills_waybill_kind_check
  CHECK (waybill_kind IN ('reimbursement','procurement','sales','hr_leave'));

-- 2. hr_leave detail table. waybill_id is the canonical id (WB-YYYY-NNNNNN).
-- Lives in the folio schema (same as waybills, users) so unqualified
-- references resolve consistently under the default search_path.
CREATE TABLE IF NOT EXISTS folio.hr_leave (
  waybill_id        text PRIMARY KEY REFERENCES folio.waybills(id) ON DELETE CASCADE,
  employee_id       int  NOT NULL REFERENCES folio.users(id),
  leave_type        text NOT NULL CHECK (leave_type IN ('sick','annual','personal')),
  start_date        date NOT NULL,
  end_date          date NOT NULL,
  days              numeric(3,1) NOT NULL CHECK (days > 0),
  reason            text,
  medical_cert_note text
);

CREATE INDEX IF NOT EXISTS idx_hr_leave_employee ON folio.hr_leave(employee_id);
CREATE INDEX IF NOT EXISTS idx_hr_leave_dates    ON folio.hr_leave(start_date, end_date);

COMMIT;
