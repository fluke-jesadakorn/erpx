-- Staff job-grade level (1 = highest executive ... 5 = staff/officer).
-- Single source of truth lives in roles.default_staff_level.
-- users.staff_level is a per-user override; NULL means "use role default".
--
-- Additive only. Safe to re-run.

-- 1. users.staff_level column.
ALTER TABLE users
  ADD COLUMN IF NOT EXISTS staff_level SMALLINT;

DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM pg_constraint WHERE conname = 'users_staff_level_range'
  ) THEN
    ALTER TABLE users
      ADD CONSTRAINT users_staff_level_range
      CHECK (staff_level IS NULL OR staff_level BETWEEN 1 AND 5);
  END IF;
END$$;

CREATE INDEX IF NOT EXISTS idx_users_staff_level
  ON users(staff_level) WHERE staff_level IS NOT NULL;

-- 2. roles.default_staff_level column.
ALTER TABLE roles
  ADD COLUMN IF NOT EXISTS default_staff_level SMALLINT;

DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM pg_constraint WHERE conname = 'roles_default_staff_level_range'
  ) THEN
    ALTER TABLE roles
      ADD CONSTRAINT roles_default_staff_level_range
      CHECK (default_staff_level BETWEEN 1 AND 5);
  END IF;
END$$;

-- 3. Reconcile roles.default_staff_level to the canonical map.
--    Mirror of app/src/lib/permissions.ts ROLE_PERMISSIONS[*].defaultStaffLevel.
--    Unconditional so re-running picks up mapping revisions.
UPDATE roles SET default_staff_level = CASE name
  WHEN 'ceo'                  THEN 1
  WHEN 'cfo'                  THEN 2
  WHEN 'admin'                THEN 2
  WHEN 'hr_manager'           THEN 3
  WHEN 'accounting_manager'   THEN 3
  WHEN 'head_of_department'   THEN 3
  WHEN 'manager'              THEN 3
  WHEN 'account_supervisor'   THEN 4
  WHEN 'supervisor'           THEN 4
  WHEN 'account_officer'      THEN 5
  WHEN 'accountant'           THEN 5
  WHEN 'hr'                   THEN 5
  WHEN 'it'                   THEN 5
  WHEN 'staff'                THEN 5
  ELSE 5
END;

-- 4. Backfill users.staff_level from role default where NULL.
UPDATE users u
SET staff_level = r.default_staff_level
FROM roles r
WHERE u.role_id = r.id AND u.staff_level IS NULL;

-- 5. Reset all users to follow the new role default.
--    Only safe when no manual per-user overrides exist.
--    Skip individual rows whose staff_level is still NULL after step 4.
UPDATE users u
SET staff_level = r.default_staff_level
FROM roles r
WHERE u.role_id = r.id
  AND u.staff_level IS NOT NULL
  AND u.staff_level <> r.default_staff_level;