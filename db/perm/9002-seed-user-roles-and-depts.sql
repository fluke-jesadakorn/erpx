-- 9002-seed-user-roles-and-depts.sql
-- Assign persona role-ids + department memberships to each user.
-- Department membership = a 'user:dept:<id>::allow' row in perm.user_permissions.

BEGIN;

-- Persona role bindings.
INSERT INTO perm.user_roles (user_id, role_id, granted_by)
SELECT u.id, m.role_id, 'seed'
  FROM users u
  JOIN (VALUES
    ('EMP006', 'ceo::1'),
    ('EMP005', 'cfo::2'),
    ('EMP019', 'finance::2'),
    ('EMP002', 'manager::3'),
    ('EMP008', 'manager::3'),
    ('EMP015', 'hr_manager::3'),
    ('EMP021', 'manager::3'),
    ('EMP023', 'manager::3'),
    ('EMP004', 'accounting_manager::3'),
    ('EMP013', 'manager::3'),
    ('EMP017', 'supervisor::4'),
    ('EMP018', 'account_supervisor::4'),
    ('EMP020', 'supervisor::4'),
    ('EMP022', 'supervisor::4'),
    ('EMP024', 'supervisor::4'),
    ('EMP001', 'officer::5'),
    ('EMP007', 'officer::5'),
    ('EMP010', 'officer::5'),
    ('EMP016', 'hr::5'),
    ('EMP025', 'officer::5'),
    ('IT001',  'it::2'),
    ('EMP003', 'account_officer::5'),
    ('EMP009', 'account_officer::5'),
    ('EMP012', 'account_officer::5'),
    ('EMP029', 'it::2')
  ) AS m(code, role_id) ON m.code = u.employee_code
ON CONFLICT DO NOTHING;

-- Ensure the 'user:dept:<id>::allow' permissions exist in the catalog.
INSERT INTO perm.permissions (id, description) VALUES
  ('user:dept:development::allow', 'Department membership: Development'),
  ('user:dept:executive::allow',   'Department membership: Executive'),
  ('user:dept:finance-2::allow',   'Department membership: Finance & Account'),
  ('user:dept:marketing::allow',   'Department membership: Marketing'),
  ('user:dept:it::allow',          'Department membership: IT'),
  ('user:dept:hr-2::allow',        'Department membership: HR')
ON CONFLICT (id) DO NOTHING;

-- Department membership via user_permissions.
-- (ON CONFLICT can't be used because the EXCLUDE constraint is DEFERRABLE.)
INSERT INTO perm.user_permissions (user_id, permission_id, granted_by, reason)
SELECT u.id, 'user:dept:' || m.dept_id || '::allow', 'seed', 'department binding'
  FROM users u
  JOIN (VALUES
    ('EMP006', 'executive'),
    ('EMP005', 'executive'),
    ('EMP019', 'finance-2'),
    ('EMP002', 'development'),
    ('EMP008', 'finance-2'),
    ('EMP015', 'hr-2'),
    ('EMP021', 'marketing'),
    ('EMP023', 'it'),
    ('EMP004', 'finance-2'),
    ('EMP013', 'finance-2'),
    ('EMP017', 'development'),
    ('EMP018', 'finance-2'),
    ('EMP020', 'marketing'),
    ('EMP022', 'it'),
    ('EMP024', 'hr-2'),
    ('EMP001', 'development'),
    ('EMP007', 'marketing'),
    ('EMP010', 'marketing'),
    ('EMP016', 'hr-2'),
    ('EMP025', 'finance-2'),
    ('IT001',  'it'),
    ('EMP003', 'finance-2'),
    ('EMP009', 'finance-2'),
    ('EMP012', 'finance-2'),
    ('EMP029', 'it')
  ) AS m(code, dept_id) ON m.code = u.employee_code
 WHERE NOT EXISTS (
   SELECT 1 FROM perm.user_permissions up
    WHERE up.user_id = u.id
      AND up.permission_id = 'user:dept:' || m.dept_id || '::allow'
      AND up.revoked_at IS NULL
 );

COMMIT;
