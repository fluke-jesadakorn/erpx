BEGIN;

SET LOCAL search_path TO folio, public;

CREATE TEMP TABLE org_seed_users (
  employee_code text PRIMARY KEY,
  fullname text NOT NULL,
  department_id text NOT NULL,
  role_id text NOT NULL
) ON COMMIT DROP;

INSERT INTO org_seed_users (employee_code, fullname, department_id, role_id) VALUES
  ('DEV-IT-MGR', 'IT Manager (Development)', 'it', 'it_manager'),
  ('DEV-IT-SUP', 'IT Supervisor (Development)', 'it', 'it_supervisor'),
  ('DEV-IT-OFF', 'IT Officer (Development)', 'it', 'it_officer'),
  ('DEV-HR-MGR', 'HR Manager (Development)', 'hr', 'hr_manager'),
  ('DEV-HR-SUP', 'HR Supervisor (Development)', 'hr', 'hr_supervisor'),
  ('DEV-HR-OFF', 'HR Officer (Development)', 'hr', 'hr_officer'),
  ('DEV-ACC-MGR', 'Accounting Manager (Development)', 'accounting', 'accounting_manager'),
  ('DEV-ACC-SUP', 'Accounting Supervisor (Development)', 'accounting', 'accounting_supervisor'),
  ('DEV-ACC-OFF', 'Accounting Officer (Development)', 'accounting', 'accounting_officer'),
  ('DEV-CFO', 'Chief Financial Officer (Development)', 'finance', 'cfo'),
  ('DEV-FIN-MGR', 'Financial Manager (Development)', 'finance', 'finance_manager'),
  ('DEV-FIN-SUP', 'Financial Supervisor (Development)', 'finance', 'finance_supervisor'),
  ('DEV-FIN-OFF', 'Financial Officer (Development)', 'finance', 'finance_officer'),
  ('DEV-CEO', 'Chief Executive Officer (Development)', 'executive', 'ceo');

SELECT setval(
  pg_get_serial_sequence('folio.users', 'id'),
  COALESCE((SELECT max(id) FROM users), 0),
  true
);

INSERT INTO users (employee_code, fullname, is_active, hired_at, position, job_description, dept_label)
SELECT s.employee_code,
       s.fullname,
       true,
       CURRENT_DATE,
       r.display_name,
       r.description,
       d.display_name
  FROM org_seed_users s
  JOIN perm.roles r ON r.id = s.role_id AND r.kind = 'hierarchy' AND r.department_id = s.department_id
  JOIN perm.departments d ON d.id = s.department_id
ON CONFLICT (employee_code) DO UPDATE
  SET fullname = EXCLUDED.fullname,
      is_active = true,
      position = EXCLUDED.position,
      job_description = EXCLUDED.job_description,
      dept_label = EXCLUDED.dept_label;

DELETE FROM perm.user_roles ur
 USING users u, org_seed_users s
 WHERE ur.user_id = u.id
   AND u.employee_code = s.employee_code;

DELETE FROM perm.user_departments ud
 USING users u, org_seed_users s
 WHERE ud.user_id = u.id
   AND u.employee_code = s.employee_code;

DELETE FROM perm.user_permissions up
 USING users u, org_seed_users s
 WHERE up.user_id = u.id
   AND u.employee_code = s.employee_code
   AND up.permission_id LIKE 'user:dept:%::allow';

INSERT INTO perm.user_departments (user_id, department_id, assigned_by)
SELECT u.id, s.department_id, NULL
  FROM users u
  JOIN org_seed_users s ON s.employee_code = u.employee_code;

INSERT INTO perm.user_roles (user_id, role_id, role_kind, granted_by)
SELECT u.id, s.role_id, 'hierarchy', 'org-user-seed-2026-07-29'
  FROM users u
  JOIN org_seed_users s ON s.employee_code = u.employee_code;

INSERT INTO perm.user_permissions (user_id, permission_id, granted_by, reason)
SELECT u.id,
       'user:dept:' || s.department_id || '::allow',
       'org-user-seed-2026-07-29',
       'Development role account department binding'
  FROM users u
  JOIN org_seed_users s ON s.employee_code = u.employee_code;

UPDATE perm.departments d
   SET head_user_id = u.id,
       updated_at = now()
  FROM users u
  JOIN org_seed_users s ON s.employee_code = u.employee_code
 WHERE d.id = s.department_id
   AND s.role_id IN ('it_manager', 'hr_manager', 'accounting_manager', 'cfo', 'ceo');

INSERT INTO perm.audit (kind, actor, target)
SELECT 'org.users.seed',
       'migration',
       jsonb_build_object(
         'employee_code', s.employee_code,
         'user_id', u.id,
         'department_id', s.department_id,
         'role_id', s.role_id
       )
  FROM org_seed_users s
  JOIN users u ON u.employee_code = s.employee_code;

DO $do$
BEGIN
  IF (SELECT count(*) FROM org_seed_users) <> 14 THEN
    RAISE EXCEPTION 'Expected fourteen development seed users';
  END IF;
  IF (
    SELECT count(*)
      FROM users u
      JOIN org_seed_users s ON s.employee_code = u.employee_code
     WHERE u.is_active IS TRUE
  ) <> 14 THEN
    RAISE EXCEPTION 'Expected all development seed users to be active';
  END IF;
  IF (
    SELECT count(*)
      FROM perm.user_roles ur
      JOIN users u ON u.id = ur.user_id
      JOIN org_seed_users s ON s.employee_code = u.employee_code
     WHERE ur.role_id = s.role_id AND ur.role_kind = 'hierarchy'
  ) <> 14 THEN
    RAISE EXCEPTION 'Expected one hierarchy role binding per development seed user';
  END IF;
  IF (
    SELECT count(*)
      FROM perm.user_departments ud
      JOIN users u ON u.id = ud.user_id
      JOIN org_seed_users s ON s.employee_code = u.employee_code
     WHERE ud.department_id = s.department_id
  ) <> 14 THEN
    RAISE EXCEPTION 'Expected one department binding per development seed user';
  END IF;
  IF (SELECT count(*) FROM perm.departments WHERE head_user_id IS NOT NULL) <> 5 THEN
    RAISE EXCEPTION 'Expected all five departments to have a seeded head';
  END IF;
END
$do$;

COMMIT;
