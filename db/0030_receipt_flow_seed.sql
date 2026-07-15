-- Folio — Receipt-flow user seed.
-- Rebuilds the user roster to a clean (dept x level) matrix so the
-- 10-stage receipt chain can be exercised end-to-end from any dept.
--
-- Note: the trigger perm_ur_sync_dept_cache on perm.user_roles syncs
-- users.dept_group_id from the user's dept rows. So we must:
--   1) DELETE dept-as-persona duplicates
--   2) TRUNCATE users
--   3) INSERT users (dept_group_id can be set; trigger will overwrite)
--   4) INSERT dept bindings into perm.user_roles (trigger syncs dept_group_id)
--   5) INSERT persona bindings into perm.user_roles

BEGIN;

-- 1. Strip dept-as-persona duplicates from perm.user_roles.
DELETE FROM perm.user_roles
 WHERE role_id IN (
   SELECT id FROM perm.roles WHERE kind = 'department'
 );

-- 2. Wipe all users. TRUNCATE CASCADE bypasses all FK types.
TRUNCATE users CASCADE;

-- 3. Department declarations (idempotent).
INSERT INTO perm.roles (id, display_name, kind, level, sort_order) VALUES
  ('dept-development', 'Development', 'department', 5, 150),
  ('dept-executive', 'Executive', 'department', 5, 160),
  ('dept-finance-2', 'Finance & Account', 'department', 5, 170),
  ('dept-marketing', 'Marketing', 'department', 5, 180),
  ('dept-it', 'IT', 'department', 5, 190),
  ('dept-hr-2', 'HR', 'department', 5, 200)
ON CONFLICT (id) DO UPDATE SET
  display_name = EXCLUDED.display_name,
  sort_order = EXCLUDED.sort_order;

-- 4. Persona declarations (idempotent, no-op when already present).
INSERT INTO perm.roles (id, display_name, kind, level, sort_order) VALUES
  ('staff', 'Staff Officer', 'persona', 5, 200),
  ('hr', 'HR Officer', 'persona', 5, 202),
  ('it', 'IT Officer', 'persona', 5, 203),
  ('finance', 'Finance', 'persona', 2, 204),
  ('account_supervisor', 'Accounting Supervisor', 'persona', 4, 210),
  ('account_officer', 'Accounting Officer', 'persona', 5, 211),
  ('supervisor', 'Supervisor', 'persona', 4, 212),
  ('manager', 'Manager', 'persona', 3, 220),
  ('hr_manager', 'HR Manager', 'persona', 3, 221),
  ('accounting_manager', 'Accounting Manager', 'persona', 3, 222),
  ('admin', 'Admin', 'persona', 2, 230),
  ('cfo', 'CFO', 'persona', 2, 231),
  ('ceo', 'CEO', 'persona', 1, 232)
ON CONFLICT (id) DO NOTHING;

-- 5. User matrix — 25 rows, full (dept x level) coverage.
--    hired_at gets CURRENT_DATE so the NOT NULL constraint is satisfied.
INSERT INTO users (employee_code, fullname, dept_group_id, hired_at, is_active) VALUES
  ('EMP006', 'Charles Executive', 'dept-executive', CURRENT_DATE, TRUE),
  ('EMP005', 'Olivia Director', 'dept-executive', CURRENT_DATE, TRUE),
  ('EMP019', 'Tina Treasurer', 'dept-finance-2', CURRENT_DATE, TRUE),
  ('EMP002', 'Sarah Approver', 'dept-development', CURRENT_DATE, TRUE),
  ('EMP008', 'David Approver', 'dept-finance-2', CURRENT_DATE, TRUE),
  ('EMP015', 'Patricia Manager', 'dept-hr-2', CURRENT_DATE, TRUE),
  ('EMP021', 'Nadia Marketing', 'dept-marketing', CURRENT_DATE, TRUE),
  ('EMP023', 'Oliver IT-Manager', 'dept-it', CURRENT_DATE, TRUE),
  ('EMP004', 'Emily Manager', 'dept-finance-2', CURRENT_DATE, TRUE),
  ('EMP013', 'Michael Manager', 'dept-finance-2', CURRENT_DATE, TRUE),
  ('EMP017', 'Steven Supervisor', 'dept-development', CURRENT_DATE, TRUE),
  ('EMP018', 'Andrew Supervisor', 'dept-finance-2', CURRENT_DATE, TRUE),
  ('EMP020', 'Marcus Marketing', 'dept-marketing', CURRENT_DATE, TRUE),
  ('EMP022', 'Iris IT-Supervisor', 'dept-it', CURRENT_DATE, TRUE),
  ('EMP024', 'Rita HR-Supervisor', 'dept-hr-2', CURRENT_DATE, TRUE),
  ('EMP001', 'John Staff', 'dept-development', CURRENT_DATE, TRUE),
  ('EMP007', 'Lisa Staff', 'dept-marketing', CURRENT_DATE, TRUE),
  ('EMP010', 'Karen Staff', 'dept-marketing', CURRENT_DATE, TRUE),
  ('EMP016', 'Jennifer Staff', 'dept-hr-2', CURRENT_DATE, TRUE),
  ('EMP025', 'Frank Finance', 'dept-finance-2', CURRENT_DATE, TRUE),
  ('IT001', 'Alex Admin', 'dept-it', CURRENT_DATE, TRUE),
  ('EMP003', 'Mark Reviewer', 'dept-finance-2', CURRENT_DATE, TRUE),
  ('EMP009', 'Robert Reviewer', 'dept-finance-2', CURRENT_DATE, TRUE),
  ('EMP012', 'Daniel Accountant', 'dept-finance-2', CURRENT_DATE, TRUE),
  ('EMP029', 'Brian Admin', 'dept-it', CURRENT_DATE, TRUE)
ON CONFLICT (employee_code) DO UPDATE SET
  fullname = EXCLUDED.fullname,
  dept_group_id = EXCLUDED.dept_group_id,
  is_active = EXCLUDED.is_active;

-- 6. Dept bindings — every user gets exactly one dept row in perm.user_roles.
--    The perm_ur_sync_dept_cache trigger populates users.dept_group_id from
--    this row. Insert with granted_by='seed' so it sorts first per the
--    ORDER BY ur.granted_at ASC clause in the trigger.
INSERT INTO perm.user_roles (user_id, role_id, granted_by)
SELECT u.id, u.dept_group_id, 'seed'
FROM users u
WHERE u.dept_group_id IS NOT NULL
ON CONFLICT DO NOTHING;

-- 7. Persona bindings — single batched insert via VALUES.
INSERT INTO perm.user_roles (user_id, role_id, granted_by)
SELECT u.id, p.id, 'seed'
FROM users u
JOIN (VALUES
  ('EMP006', 'ceo'),
  ('EMP005', 'cfo'),
  ('EMP019', 'finance'),
  ('EMP002', 'manager'),
  ('EMP008', 'manager'),
  ('EMP015', 'hr_manager'),
  ('EMP021', 'manager'),
  ('EMP023', 'manager'),
  ('EMP004', 'accounting_manager'),
  ('EMP013', 'manager'),
  ('EMP017', 'supervisor'),
  ('EMP018', 'account_supervisor'),
  ('EMP020', 'supervisor'),
  ('EMP022', 'supervisor'),
  ('EMP024', 'supervisor'),
  ('EMP001', 'staff'),
  ('EMP007', 'staff'),
  ('EMP010', 'staff'),
  ('EMP016', 'hr'),
  ('EMP025', 'staff'),
  ('IT001', 'it'),
  ('EMP003', 'account_officer'),
  ('EMP009', 'account_officer'),
  ('EMP012', 'account_officer'),
  ('EMP029', 'it')
) AS m(code, persona) ON m.code = u.employee_code
JOIN perm.roles p ON p.id = m.persona AND p.kind = 'persona'
ON CONFLICT DO NOTHING;

-- 9. Reset sequences so future inserts use clean ids.
SELECT setval('users_id_seq', GREATEST((SELECT MAX(id) FROM users), 1));

COMMIT;