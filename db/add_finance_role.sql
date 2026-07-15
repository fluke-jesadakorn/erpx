-- v6 schema: finance (treasury) role for the disbursement approval stage.
-- Additive only. Safe to re-run.

SELECT setval('roles_id_seq', GREATEST((SELECT MAX(id) FROM roles), 1));

INSERT INTO roles (name) VALUES ('finance')
ON CONFLICT (name) DO NOTHING;

SELECT setval('roles_id_seq', GREATEST((SELECT MAX(id) FROM roles), 1));

INSERT INTO users (employee_code, fullname, role_id, department, department_id, is_active)
SELECT 'EMP019', 'Tina Treasurer', r.id, 'Finance & Account',
       (SELECT id FROM departments WHERE code='FIN'), TRUE
FROM roles r WHERE r.name='finance'
ON CONFLICT (employee_code) DO UPDATE SET
  fullname    = EXCLUDED.fullname,
  role_id     = EXCLUDED.role_id,
  department  = EXCLUDED.department,
  department_id = EXCLUDED.department_id,
  is_active   = TRUE;

UPDATE users
SET reports_to_user_id = (SELECT id FROM users WHERE employee_code='EMP004')
WHERE employee_code='EMP019' AND reports_to_user_id IS NULL;

UPDATE roles SET default_staff_level = 5 WHERE name='finance' AND default_staff_level IS NULL;
UPDATE users  u
SET staff_level = r.default_staff_level
FROM roles r
WHERE u.role_id = r.id AND u.staff_level IS NULL AND r.name='finance';
