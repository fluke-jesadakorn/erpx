-- v4 schema: HR + Organization Chart + delegation
-- Additive only: never drops columns or breaks v1/v2/v3 schemas.
-- Safe to re-run.

-- 1. New roles: hr (read-only directory) + hr_manager (full CRUD).
SELECT setval('roles_id_seq', GREATEST((SELECT MAX(id) FROM roles), 1));
INSERT INTO roles (name) VALUES ('hr')         ON CONFLICT (name) DO NOTHING;
INSERT INTO roles (name) VALUES ('hr_manager') ON CONFLICT (name) DO NOTHING;
SELECT setval('roles_id_seq', GREATEST((SELECT MAX(id) FROM roles), 1));

-- 2. Add scoping columns to users.
ALTER TABLE users
  ADD COLUMN IF NOT EXISTS department_id INT REFERENCES departments(id) ON DELETE SET NULL,
  ADD COLUMN IF NOT EXISTS reports_to_user_id INT REFERENCES users(id) ON DELETE SET NULL,
  ADD COLUMN IF NOT EXISTS is_active BOOLEAN DEFAULT TRUE;

-- CHECK constraint preventing self-cycle in reports_to. (Idempotent via DO block.)
DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM pg_constraint WHERE conname = 'users_no_self_report'
  ) THEN
    ALTER TABLE users
      ADD CONSTRAINT users_no_self_report
      CHECK (id IS NULL OR reports_to_user_id IS NULL OR id <> reports_to_user_id);
  END IF;
END$$;

CREATE INDEX IF NOT EXISTS idx_users_reports_to ON users(reports_to_user_id);
CREATE INDEX IF NOT EXISTS idx_users_dept_id     ON users(department_id);
CREATE INDEX IF NOT EXISTS idx_users_active      ON users(is_active) WHERE is_active;

-- 3. Backfill department_id from existing free-text department (matches by code OR name).
UPDATE users u
SET department_id = d.id
FROM departments d
WHERE u.department_id IS NULL
  AND (u.department = d.code OR u.department = d.name);

-- 4. Seed HR users (idempotent).
INSERT INTO users (employee_code, fullname, role_id, department, department_id, is_active)
SELECT 'EMP015', 'Patricia Manager', r.id, 'Human Resource',
       (SELECT id FROM departments WHERE code='HR'), TRUE
FROM roles r WHERE r.name='hr_manager'
ON CONFLICT (employee_code) DO UPDATE SET
  fullname = EXCLUDED.fullname,
  role_id  = EXCLUDED.role_id,
  department_id = EXCLUDED.department_id,
  is_active = TRUE;

INSERT INTO users (employee_code, fullname, role_id, department, department_id, is_active)
SELECT 'EMP016', 'Jennifer Staff', r.id, 'Human Resource',
       (SELECT id FROM departments WHERE code='HR'), TRUE
FROM roles r WHERE r.name='hr'
ON CONFLICT (employee_code) DO UPDATE SET
  fullname = EXCLUDED.fullname,
  role_id  = EXCLUDED.role_id,
  department_id = EXCLUDED.department_id,
  is_active = TRUE;

-- 5. Seed reports_to chains (only fills if NULL — never overwrites real org data).
--    Engineering Head (EMP002) ← every staff in Engineering/Development.
UPDATE users child
SET reports_to_user_id = (SELECT id FROM users WHERE employee_code='EMP002')
WHERE child.reports_to_user_id IS NULL
  AND child.employee_code IN ('EMP001')
  AND child.department_id IN (
    SELECT id FROM departments WHERE code IN ('ENG','DEV')
  );

-- Sales Manager (EMP008) ← Sales staff (EMP007).
UPDATE users child
SET reports_to_user_id = (SELECT id FROM users WHERE employee_code='EMP008')
WHERE child.reports_to_user_id IS NULL
  AND child.employee_code = 'EMP007';

-- HR Manager (EMP015) ← HR Officer (EMP016).
UPDATE users child
SET reports_to_user_id = (SELECT id FROM users WHERE employee_code='EMP015')
WHERE child.reports_to_user_id IS NULL
  AND child.employee_code = 'EMP016';

-- 6. HR Manager (EMP015) ← reports to CEO (EMP006) for top of tree.
UPDATE users
SET reports_to_user_id = (SELECT id FROM users WHERE employee_code='EMP006')
WHERE employee_code = 'EMP015' AND reports_to_user_id IS NULL;
