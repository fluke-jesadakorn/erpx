BEGIN;

INSERT INTO perm.departments(id, display_name, is_system)
VALUES ('sales', 'Sales Department', true)
ON CONFLICT (id) DO UPDATE SET display_name = excluded.display_name, is_system = true;

INSERT INTO perm.permissions(id, description)
VALUES ('user:dept:sales::allow', 'Sales department membership')
ON CONFLICT (id) DO UPDATE SET description = excluded.description;

INSERT INTO perm.roles(id, display_name, description, kind, rank, is_system, sort_order, department_id) VALUES
  ('sales_manager', 'Sales Manager', 'Owns sales authorization and department expense approval', 'hierarchy', 3, true, 610, 'sales'),
  ('sales_supervisor', 'Sales Supervisor', 'Reviews sales orders and verifies department expenses', 'hierarchy', 4, true, 620, 'sales'),
  ('sales_officer', 'Sales Officer', 'Creates sales orders and employee expense claims', 'hierarchy', 5, true, 630, 'sales')
ON CONFLICT (id, kind) DO UPDATE
  SET display_name = excluded.display_name,
      description = excluded.description,
      rank = excluded.rank,
      department_id = excluded.department_id,
      is_system = true;

INSERT INTO perm.department_permissions(department_id, permission_id, granted_by, significance)
SELECT 'sales', p, 'local-reference-seed', true
  FROM unnest(ARRAY[
    'ai:chat:use::allow',
    'finance:expense:create::allow',
    'finance:expense:submit::allow',
    'finance:expense:update::allow',
    'finance:expense:view_own::allow',
    'finance:sales:submit::allow',
    'org:dept:read::allow',
    'org:tree:view::allow',
    'system:authenticated:view::allow',
    'tile:chat:view::allow',
    'tile:customers:view::allow',
    'tile:expense:view::allow',
    'tile:inbox:view::allow',
    'tile:sales:view::allow',
    'user:directory:read::allow'
  ]) p
ON CONFLICT (department_id, permission_id) DO UPDATE
  SET granted_by = excluded.granted_by, significance = excluded.significance;

INSERT INTO perm.role_permissions(role_id, role_kind, permission_id, granted_by, significance)
SELECT x.role_id, 'hierarchy', x.permission_id, 'local-reference-seed', true
  FROM (VALUES
    ('sales_manager', 'finance:expense:approve::allow'),
    ('sales_manager', 'finance:expense:department_approve::allow'),
    ('sales_manager', 'stage:department_approval:act::allow'),
    ('sales_manager', 'stage:dept_authorization:act::allow'),
    ('sales_manager', 'stage:so_dept_approval:act::allow'),
    ('sales_supervisor', 'finance:expense:review::allow'),
    ('sales_supervisor', 'stage:dept_verification:act::allow'),
    ('sales_supervisor', 'stage:so_sales_review:act::allow'),
    ('sales_officer', 'stage:submission:act::allow'),
    ('sales_officer', 'stage:so_draft:act::allow')
  ) x(role_id, permission_id)
ON CONFLICT (role_id, permission_id) DO UPDATE
  SET granted_by = excluded.granted_by, significance = excluded.significance;

INSERT INTO perm.role_permissions(role_id, role_kind, permission_id, granted_by, significance)
SELECT r.id, 'hierarchy', p.permission_id, 'local-reference-seed', true
  FROM perm.roles r
  CROSS JOIN (VALUES
    ('finance:journal:prepare::allow'),
    ('finance:journal:approve::allow')
  ) p(permission_id)
 WHERE r.id IN ('cfo', 'finance_manager', 'finance_supervisor', 'finance_officer')
ON CONFLICT (role_id, permission_id) DO UPDATE
  SET granted_by = excluded.granted_by, significance = excluded.significance;

CREATE TEMP TABLE local_users (
  employee_code text PRIMARY KEY,
  fullname text NOT NULL,
  department_id text NOT NULL,
  role_id text NOT NULL
) ON COMMIT DROP;

INSERT INTO local_users(employee_code, fullname, department_id, role_id) VALUES
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
  ('DEV-SALES-MGR', 'Sales Manager (Development)', 'sales', 'sales_manager'),
  ('DEV-SALES-SUP', 'Sales Supervisor (Development)', 'sales', 'sales_supervisor'),
  ('DEV-SALES-OFF', 'Sales Officer (Development)', 'sales', 'sales_officer'),
  ('DEV-CEO', 'Chief Executive Officer (Development)', 'executive', 'ceo');

INSERT INTO folio.users(employee_code, fullname, is_active, hired_at, position, job_description, dept_label)
SELECT s.employee_code, s.fullname, true, current_date, r.display_name, coalesce(r.description, ''), d.display_name
  FROM local_users s
  JOIN perm.roles r ON r.id = s.role_id AND r.kind = 'hierarchy' AND r.department_id = s.department_id
  JOIN perm.departments d ON d.id = s.department_id
ON CONFLICT (employee_code) DO UPDATE
  SET fullname = excluded.fullname,
      is_active = true,
      position = excluded.position,
      job_description = excluded.job_description,
      dept_label = excluded.dept_label;

DELETE FROM perm.user_roles ur
 USING folio.users u, local_users s
 WHERE ur.user_id = u.id AND u.employee_code = s.employee_code;

DELETE FROM perm.user_departments ud
 USING folio.users u, local_users s
 WHERE ud.user_id = u.id AND u.employee_code = s.employee_code;

DELETE FROM perm.user_permissions up
 USING folio.users u, local_users s
 WHERE up.user_id = u.id
   AND u.employee_code = s.employee_code
   AND up.permission_id LIKE 'user:dept:%::allow';

INSERT INTO perm.user_departments(user_id, department_id, assigned_by)
SELECT u.id, s.department_id, NULL
  FROM folio.users u
  JOIN local_users s ON s.employee_code = u.employee_code;

INSERT INTO perm.user_roles(user_id, role_id, role_kind, granted_by)
SELECT u.id, s.role_id, 'hierarchy', 'local-reference-seed'
  FROM folio.users u
  JOIN local_users s ON s.employee_code = u.employee_code;

INSERT INTO perm.user_permissions(user_id, permission_id, granted_by, reason)
SELECT u.id, 'user:dept:' || s.department_id || '::allow', 'local-reference-seed', 'Local reference identity'
  FROM folio.users u
  JOIN local_users s ON s.employee_code = u.employee_code;

UPDATE perm.departments d
   SET head_user_id = u.id,
       updated_at = now()
  FROM folio.users u
  JOIN local_users s ON s.employee_code = u.employee_code
 WHERE d.id = s.department_id
   AND s.role_id IN ('it_manager', 'hr_manager', 'accounting_manager', 'cfo', 'sales_manager', 'ceo');

DO $assert$
BEGIN
  IF (SELECT count(*) FROM local_users) <> 17 THEN RAISE EXCEPTION 'Expected 17 local users'; END IF;
  IF (
    SELECT count(*)
      FROM folio.users u
      JOIN local_users s ON s.employee_code = u.employee_code
     WHERE u.is_active
  ) <> 17 THEN RAISE EXCEPTION 'Expected 17 active local users'; END IF;
  IF (
    SELECT count(*)
      FROM perm.user_roles ur
      JOIN folio.users u ON u.id = ur.user_id
      JOIN local_users s ON s.employee_code = u.employee_code
     WHERE ur.role_id = s.role_id
  ) <> 17 THEN RAISE EXCEPTION 'Expected 17 local role bindings'; END IF;
END
$assert$;

COMMIT;
