\set ON_ERROR_STOP on

SELECT count(*) = 9 AS department_count_ok
  FROM perm.departments;

SELECT count(*) FILTER (WHERE kind = 'hierarchy') = 7
       AND count(*) FILTER (WHERE kind = 'system') = 1 AS role_count_ok
  FROM perm.roles;

SELECT count(*) = 0 AS orphan_role_grants_ok
  FROM perm.role_permissions rp
  LEFT JOIN perm.roles r ON r.id = rp.role_id AND r.kind = rp.role_kind
 WHERE r.id IS NULL;

SELECT count(*) = 0 AS orphan_user_roles_ok
  FROM perm.user_roles ur
  LEFT JOIN perm.roles r ON r.id = ur.role_id AND r.kind = ur.role_kind
 WHERE r.id IS NULL;

SELECT count(*) = 0 AS orphan_departments_ok
  FROM perm.user_departments ud
  LEFT JOIN perm.departments d ON d.id = ud.department_id
 WHERE d.id IS NULL;

SELECT count(*) = 0 AS bootstrap_count_ok
  FROM (
    SELECT count(*) AS n FROM perm.user_departments
  ) x
 WHERE n <> 9;

SELECT count(*) = 0 AS invalid_permissions_ok
  FROM perm.permissions
 WHERE id !~ '^[a-z][a-z0-9_]*:[a-z][a-z0-9_]*:[a-z][a-z0-9_]*(?::[a-zA-Z0-9_*-]+)?::(allow|deny)$';

SELECT count(*) = 0 AS non_bootstrap_access_reset_ok
  FROM perm.user_departments
 WHERE department_id NOT IN ('hr', 'it');

SELECT count(*) = 0 AS duplicate_access_ok
  FROM (
    SELECT user_id FROM perm.user_departments GROUP BY user_id HAVING count(*) > 1
    UNION ALL
    SELECT user_id FROM perm.user_roles WHERE role_kind = 'hierarchy' GROUP BY user_id HAVING count(*) > 1
    UNION ALL
    SELECT user_id FROM perm.user_roles WHERE role_kind = 'system' GROUP BY user_id HAVING count(*) > 1
  ) d;

SELECT count(*) = 0 AS expense_stage_migration_ok
  FROM expenses
 WHERE status NOT IN (
   'draft', 'submission', 'department_approval', 'accounting_review',
   'accounting_approval', 'executive_approval', 'payment', 'settlement',
   'completed', 'rejected'
 );

SELECT count(*) = 0 AS payment_uniqueness_ok
  FROM (
    SELECT waybill_id FROM expense_payments GROUP BY waybill_id HAVING count(*) > 1
    UNION ALL
    SELECT expense_id::text FROM expense_payments GROUP BY expense_id HAVING count(*) > 1
    UNION ALL
    SELECT slip_id::text FROM expense_payments GROUP BY slip_id HAVING count(*) > 1
  ) p;

WITH chain AS (
  SELECT waybill_id, sequence, previous_event_id,
         row_number() OVER (PARTITION BY waybill_id ORDER BY sequence) AS expected_sequence,
         lag(id) OVER (PARTITION BY waybill_id ORDER BY sequence) AS expected_previous
    FROM waybill_events
)
SELECT count(*) = 0 AS waybill_event_chain_ok
  FROM chain
 WHERE sequence <> expected_sequence
    OR previous_event_id IS DISTINCT FROM expected_previous;
