SET search_path TO folio, public;

SELECT count(*) = 14 AS active_development_users
  FROM users
 WHERE is_active
   AND employee_code LIKE 'DEV-%';

SELECT count(*) = 14 AS seeded_role_bindings
  FROM (
    SELECT user_id FROM perm.user_roles
  ) x;

SELECT count(*) = 14 AS seeded_department_bindings
  FROM perm.user_departments;

SELECT count(*) = 14 AS seeded_membership_permissions
  FROM perm.user_permissions
 WHERE permission_id LIKE 'user:dept:%::allow'
   AND revoked_at IS NULL;

SELECT count(*) = 5 AS department_count_ok
  FROM perm.departments;

SELECT count(*) = 14 AS role_count_ok
  FROM perm.roles;

WITH expected(id, department_id, rank) AS (
  VALUES
    ('it_manager', 'it', 3),
    ('it_supervisor', 'it', 4),
    ('it_officer', 'it', 5),
    ('hr_manager', 'hr', 3),
    ('hr_supervisor', 'hr', 4),
    ('hr_officer', 'hr', 5),
    ('accounting_manager', 'accounting', 3),
    ('accounting_supervisor', 'accounting', 4),
    ('accounting_officer', 'accounting', 5),
    ('cfo', 'finance', 2),
    ('finance_manager', 'finance', 3),
    ('finance_supervisor', 'finance', 4),
    ('finance_officer', 'finance', 5),
    ('ceo', 'executive', 1)
)
SELECT count(*) = 0 AS role_relation_set_ok
  FROM (
    (SELECT id, department_id, rank FROM expected
     EXCEPT
     SELECT id, department_id, rank::integer FROM perm.roles)
    UNION ALL
    (SELECT id, department_id, rank::integer FROM perm.roles
     EXCEPT
     SELECT id, department_id, rank FROM expected)
  ) delta;

SELECT bool_and(grants > 0) AS every_role_has_permissions
  FROM (
    SELECT r.id, count(rp.permission_id) AS grants
      FROM perm.roles r
      LEFT JOIN perm.role_permissions rp ON rp.role_id = r.id
     GROUP BY r.id
  ) x;

SELECT count(*) = 5 AS every_department_has_a_head
  FROM perm.departments
 WHERE head_user_id IS NOT NULL;

SELECT NOT EXISTS (
  SELECT 1
    FROM perm.roles r
    JOIN perm.role_permissions rp ON rp.role_id = r.id
   WHERE r.department_id = 'it'
     AND rp.permission_id IN (
       'finance:expense:accounting_approve::allow',
       'stage:accounting_authorization:act::allow',
       'stage:cfo_authorization:act::allow',
       'stage:ceo_authorization:act::allow'
     )
) AS it_has_no_financial_approval_authority;

SELECT NOT EXISTS (
  SELECT 1 FROM perm.role_permissions
   WHERE permission_id = 'admin:system:bypass::allow'
) AS no_global_bypass_grant;
