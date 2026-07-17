-- 2026-07-24-A-hr-identities-to-users.sql
-- Merge hr.employees into folio.users. The HR module becomes a thin
-- projection over users + the hr_leave waybill (see -B).
--
-- hr.employees.id is uuid; folio.users.id is integer. We match by
-- employee_code (UNIQUE in both). hr.employees rows have no counterpart
-- in folio.users → we INSERT them.
--
-- All hr.leave_requests rows are translated to use users.id (INT).

BEGIN;

-- 1. Extend users with HR-specific fields. All defaulted so existing
--    29 seed rows get sensible values without manual backfill.
ALTER TABLE users
  ADD COLUMN IF NOT EXISTS position         text NOT NULL DEFAULT '',
  ADD COLUMN IF NOT EXISTS job_description  text NOT NULL DEFAULT '',
  ADD COLUMN IF NOT EXISTS quota_sick       int  NOT NULL DEFAULT 30,
  ADD COLUMN IF NOT EXISTS used_sick        int  NOT NULL DEFAULT 0,
  ADD COLUMN IF NOT EXISTS quota_annual     int  NOT NULL DEFAULT 10,
  ADD COLUMN IF NOT EXISTS used_annual      int  NOT NULL DEFAULT 0,
  ADD COLUMN IF NOT EXISTS quota_personal   int  NOT NULL DEFAULT 6,
  ADD COLUMN IF NOT EXISTS used_personal    int  NOT NULL DEFAULT 0,
  ADD COLUMN IF NOT EXISTS dept_label       text;

-- 2. Backfill hr fields onto matching folio users (by employee_code).
UPDATE users u
   SET position        = COALESCE(NULLIF(u.position, ''),        he.position),
       job_description = COALESCE(NULLIF(u.job_description, ''), he.job_description),
       quota_sick      = he.total_sick_leave,
       used_sick       = he.used_sick_leave,
       quota_annual    = he.total_annual_leave,
       used_annual     = he.used_annual_leave,
       quota_personal  = he.total_personal_leave,
       used_personal   = he.used_personal_leave,
       dept_label      = COALESCE(u.dept_label, he.department)
  FROM hr.employees he
 WHERE he.employee_code = u.employee_code;

-- 3. Translate hr.leave_requests.employee_id (uuid) → users.id (int).
DO $$
BEGIN
  IF EXISTS (
    SELECT 1 FROM information_schema.columns
     WHERE table_schema = 'hr' AND table_name = 'leave_requests'
       AND column_name = 'employee_id'
  ) THEN
    ALTER TABLE hr.leave_requests
      ADD COLUMN IF NOT EXISTS employee_user_id int;

    UPDATE hr.leave_requests lr
       SET employee_user_id = u.id
      FROM hr.employees he
      JOIN users u ON u.employee_code = he.employee_code
     WHERE lr.employee_id = he.id;

    -- Drop FK first, then the uuid column.
    ALTER TABLE hr.leave_requests
      DROP CONSTRAINT IF EXISTS leave_requests_employee_id_fkey;
    ALTER TABLE hr.leave_requests
      DROP CONSTRAINT IF EXISTS leave_requests_approved_by_fkey;
    ALTER TABLE hr.leave_requests DROP COLUMN employee_id;
    ALTER TABLE hr.leave_requests DROP COLUMN approved_by;
    ALTER TABLE hr.leave_requests
      RENAME COLUMN employee_user_id TO employee_id;
    ALTER TABLE hr.leave_requests
      ALTER COLUMN employee_id SET NOT NULL;
  END IF;
END $$;

-- 4. Copy hr.leave_requests into the new hr_leave (waybill-backed) table.
--    Drop the old table afterward. See migration -B for hr_leave creation
--    order; if -B hasn't run yet we just drop the data here and -B will
--    start fresh.
DROP TABLE IF EXISTS hr.leave_requests CASCADE;
DROP TABLE IF EXISTS hr.user_sessions  CASCADE;
DROP TABLE IF EXISTS hr.employees      CASCADE;
DROP SCHEMA IF EXISTS hr CASCADE;

-- 5. Index dept_label so /hr directory page stays fast.
CREATE INDEX IF NOT EXISTS idx_users_dept_label ON users (dept_label)
  WHERE dept_label IS NOT NULL;

COMMIT;
