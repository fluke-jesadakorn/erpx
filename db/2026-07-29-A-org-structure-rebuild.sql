BEGIN;

SET LOCAL search_path TO folio, public;

LOCK TABLE users IN SHARE ROW EXCLUSIVE MODE;
LOCK TABLE perm.roles IN ACCESS EXCLUSIVE MODE;
LOCK TABLE perm.departments IN ACCESS EXCLUSIVE MODE;

DELETE FROM auth.sessions;
DELETE FROM perm.user_roles;
DELETE FROM perm.user_departments;
DELETE FROM perm.user_permissions;

UPDATE perm.departments SET head_user_id = NULL, updated_at = now();

UPDATE users
   SET employee_code = 'ARCH-' || id,
       fullname = 'Archived User #' || id,
       line_user_id = NULL,
       is_active = false,
       position = '',
       job_description = '',
       dept_label = NULL;

DELETE FROM perm.roles;
DELETE FROM perm.departments;

DELETE FROM perm.permissions
 WHERE id LIKE 'user:dept:%::allow'
   AND id NOT IN (
     'user:dept:it::allow',
     'user:dept:hr::allow',
     'user:dept:accounting::allow',
     'user:dept:finance::allow',
     'user:dept:executive::allow'
   );

ALTER TABLE perm.roles ADD COLUMN IF NOT EXISTS department_id text;
ALTER TABLE perm.roles DROP CONSTRAINT IF EXISTS roles_department_id_fkey;
ALTER TABLE perm.roles DROP CONSTRAINT IF EXISTS roles_department_kind_check;
ALTER TABLE perm.roles
  ADD CONSTRAINT roles_department_id_fkey
  FOREIGN KEY (department_id) REFERENCES perm.departments(id) ON DELETE RESTRICT;
ALTER TABLE perm.roles
  ADD CONSTRAINT roles_department_kind_check
  CHECK ((kind = 'hierarchy' AND department_id IS NOT NULL) OR (kind = 'system' AND department_id IS NULL));
CREATE INDEX IF NOT EXISTS perm_roles_department_idx ON perm.roles(department_id, rank, sort_order);

INSERT INTO perm.departments (id, display_name, is_system) VALUES
  ('it', 'Information Technology Department', true),
  ('hr', 'Human Resources Department', true),
  ('accounting', 'Accounting Department', true),
  ('finance', 'Financial Department', true),
  ('executive', 'Executive Department', true);

INSERT INTO perm.permissions (id, description) VALUES
  ('user:dept:it::allow', 'Information Technology Department membership marker'),
  ('user:dept:hr::allow', 'Human Resources Department membership marker'),
  ('user:dept:accounting::allow', 'Accounting Department membership marker'),
  ('user:dept:finance::allow', 'Financial Department membership marker'),
  ('user:dept:executive::allow', 'Executive Department membership marker')
ON CONFLICT (id) DO UPDATE SET description = EXCLUDED.description;

INSERT INTO perm.roles
  (id, display_name, description, kind, rank, department_id, is_system, sort_order)
VALUES
  ('it_manager', 'IT Manager', 'Owns platform administration and IT operations', 'hierarchy', 3, 'it', true, 110),
  ('it_supervisor', 'IT Supervisor', 'Supervises platform operations and support', 'hierarchy', 4, 'it', true, 120),
  ('it_officer', 'IT Officer', 'Operates platform support and monitoring', 'hierarchy', 5, 'it', true, 130),
  ('hr_manager', 'HR Manager', 'Owns people operations and access assignment', 'hierarchy', 3, 'hr', true, 210),
  ('hr_supervisor', 'HR Supervisor', 'Supervises employee and leave operations', 'hierarchy', 4, 'hr', true, 220),
  ('hr_officer', 'HR Officer', 'Operates employee and leave services', 'hierarchy', 5, 'hr', true, 230),
  ('accounting_manager', 'Accounting Manager', 'Owns accounting authorization and posting', 'hierarchy', 3, 'accounting', true, 310),
  ('accounting_supervisor', 'Accounting Supervisor', 'Supervises accounting review and confirmation', 'hierarchy', 4, 'accounting', true, 320),
  ('accounting_officer', 'Accounting Officer', 'Prepares and verifies accounting work', 'hierarchy', 5, 'accounting', true, 330),
  ('cfo', 'Chief Financial Officer', 'Owns financial executive authorization', 'hierarchy', 2, 'finance', true, 410),
  ('finance_manager', 'Financial Manager', 'Owns finance operations and disbursement', 'hierarchy', 3, 'finance', true, 420),
  ('finance_supervisor', 'Financial Supervisor', 'Supervises payment and settlement operations', 'hierarchy', 4, 'finance', true, 430),
  ('finance_officer', 'Financial Officer', 'Operates payment and finance services', 'hierarchy', 5, 'finance', true, 440),
  ('ceo', 'Chief Executive Officer', 'Owns final executive authorization', 'hierarchy', 1, 'executive', true, 510);

INSERT INTO perm.role_permissions (role_id, role_kind, permission_id, granted_by)
SELECT r.id, r.kind, p.id, 'org-rebuild-2026-07-29'
  FROM perm.roles r
  JOIN perm.permissions p ON p.id IN (
    'system:authenticated:view::allow',
    'user:directory:read::allow',
    'org:tree:view::allow',
    'org:dept:read::allow',
    'ai:chat:use::allow',
    'finance:expense:create::allow',
    'finance:expense:submit::allow',
    'finance:expense:update::allow',
    'finance:expense:view_own::allow',
    'finance:pr:create::allow',
    'finance:pr:update::allow',
    'tile:chat:view::allow',
    'tile:directory:view::allow',
    'tile:expense:view::allow',
    'tile:inbox:view::allow',
    'tile:my_prs:view::allow',
    'tile:pr:view::allow'
  );

INSERT INTO perm.role_permissions (role_id, role_kind, permission_id, granted_by)
SELECT r.id, r.kind, p.id, 'org-rebuild-2026-07-29'
  FROM perm.roles r
  JOIN perm.permissions p ON p.id IN (
    'finance:expense:department_approve::allow',
    'stage:department_approval:act::allow',
    'stage:dept_verification:act::allow',
    'stage:submission:act::allow'
  )
 WHERE r.id LIKE '%_supervisor' OR r.id LIKE '%_manager';

INSERT INTO perm.role_permissions (role_id, role_kind, permission_id, granted_by)
SELECT r.id, r.kind, p.id, 'org-rebuild-2026-07-29'
  FROM perm.roles r
  JOIN perm.permissions p ON p.id IN (
    'finance:expense:claim_reassign::allow',
    'stage:dept_authorization:act::allow',
    'user:manager:set::allow',
    'user:subtree:edit::allow'
  )
 WHERE r.id LIKE '%_manager';

INSERT INTO perm.role_permissions (role_id, role_kind, permission_id, granted_by)
SELECT r.id, r.kind, p.id, 'org-rebuild-2026-07-29'
  FROM perm.roles r
  JOIN perm.permissions p ON p.id IN (
    'ai:invocation:view::allow',
    'ai:model:read::allow',
    'ai:provider:read::allow',
    'ai:section_health:view::allow',
    'ai:staff:invoke::allow',
    'ai:staff:read::allow',
    'hook:event:view::allow',
    'tile:dash_it:view::allow',
    'tile:hook:view::allow',
    'tile:settings:view::allow'
  )
 WHERE r.department_id = 'it';

INSERT INTO perm.role_permissions (role_id, role_kind, permission_id, granted_by)
SELECT r.id, r.kind, p.id, 'org-rebuild-2026-07-29'
  FROM perm.roles r
  JOIN perm.permissions p ON p.id IN (
    'access_request:request:list::allow',
    'ai:assignment:create::allow',
    'ai:assignment:read::allow',
    'ai:assignment:update::allow',
    'ai:model:create::allow',
    'ai:model:update::allow',
    'ai:provider:create::allow',
    'ai:provider:test::allow',
    'ai:provider:update::allow',
    'ai:staff:create::allow',
    'ai:staff:update::allow',
    'hook:event:replay::allow',
    'rbac:audit:view::allow',
    'rbac:matrix:view::allow',
    'rbac:role:read::allow',
    'tile:access_requests:view::allow',
    'tile:audit:view::allow',
    'tile:policy:view::allow'
  )
 WHERE r.department_id = 'it' AND r.rank <= 4;

INSERT INTO perm.role_permissions (role_id, role_kind, permission_id, granted_by)
SELECT r.id, r.kind, p.id, 'org-rebuild-2026-07-29'
  FROM perm.roles r
  JOIN perm.permissions p ON p.id IN (
    'access_request:request:resolve::allow',
    'ai:assignment:delete::allow',
    'ai:model:delete::allow',
    'ai:provider:delete::allow',
    'ai:staff:delete::allow',
    'org:auto_wire:apply::allow',
    'org:auto_wire:propose::allow',
    'org:dept:assign_head::allow',
    'org:dept_role:assign::allow',
    'org:dept_role:list::allow',
    'org:dept_role:revoke::allow',
    'rbac:department:assign::allow',
    'rbac:department:edit::allow',
    'rbac:matrix:edit::allow',
    'rbac:role:assign::allow',
    'rbac:role:edit::allow',
    'tile:departments:view::allow',
    'tile:roles:view::allow',
    'tile:tile_gates:view::allow',
    'user:dept:edit::allow',
    'user:profile:create::allow',
    'user:profile:deactivate::allow',
    'user:profile:delete::allow',
    'user:profile:update::allow',
    'user:role:assign::allow'
  )
 WHERE r.id = 'it_manager';

INSERT INTO perm.role_permissions (role_id, role_kind, permission_id, granted_by)
SELECT r.id, r.kind, p.id, 'org-rebuild-2026-07-29'
  FROM perm.roles r
  JOIN perm.permissions p ON p.id IN (
    'hr:employee:list::allow',
    'hr:employee:read::allow',
    'hr:employee:update::allow',
    'hr:export:csv::allow',
    'hr:leave:read::allow',
    'hr:leave:submit::allow',
    'hr:quota:read::allow',
    'tile:dash_staff:view::allow',
    'tile:hr:view::allow',
    'tile:hr_employees:view::allow',
    'tile:hr_leave:view::allow',
    'tile:me_leave:view::allow',
    'tile:org_chart:view::allow'
  )
 WHERE r.department_id = 'hr';

INSERT INTO perm.role_permissions (role_id, role_kind, permission_id, granted_by)
SELECT r.id, r.kind, p.id, 'org-rebuild-2026-07-29'
  FROM perm.roles r
  JOIN perm.permissions p ON p.id IN (
    'hr:analytics:view::allow',
    'hr:employee:create::allow',
    'hr:leave:approve::allow',
    'hr:leave:reject::allow',
    'hr:quota:update::allow',
    'stage:hr_authorization:act::allow',
    'stage:hr_review:act::allow',
    'tile:dash_manager:view::allow'
  )
 WHERE r.department_id = 'hr' AND r.rank <= 4;

INSERT INTO perm.role_permissions (role_id, role_kind, permission_id, granted_by)
SELECT r.id, r.kind, p.id, 'org-rebuild-2026-07-29'
  FROM perm.roles r
  JOIN perm.permissions p ON p.id IN (
    'access_request:request:list::allow',
    'access_request:request:resolve::allow',
    'hr:employee:deactivate::allow',
    'org:dept:assign_head::allow',
    'org:dept_role:assign::allow',
    'org:dept_role:list::allow',
    'org:dept_role:revoke::allow',
    'rbac:department:assign::allow',
    'rbac:matrix:view::allow',
    'rbac:role:assign::allow',
    'rbac:role:read::allow',
    'tile:access_requests:view::allow',
    'tile:departments:view::allow',
    'tile:policy:view::allow',
    'tile:roles:view::allow',
    'user:dept:edit::allow',
    'user:profile:create::allow',
    'user:profile:deactivate::allow',
    'user:profile:delete::allow',
    'user:profile:update::allow',
    'user:role:assign::allow'
  )
 WHERE r.id = 'hr_manager';

INSERT INTO perm.role_permissions (role_id, role_kind, permission_id, granted_by)
SELECT r.id, r.kind, p.id, 'org-rebuild-2026-07-29'
  FROM perm.roles r
  JOIN perm.permissions p ON p.id IN (
    'finance:expense:accounting_prepare::allow',
    'finance:expense:claim::allow',
    'finance:expense:review::allow',
    'finance:expense:view_all::allow',
    'finance:gl:view::allow',
    'finance:ledger:view::allow',
    'stage:accounting_review:act::allow',
    'stage:accounting_verification:act::allow',
    'tile:dash_am:view::allow',
    'tile:ledger:view::allow',
    'tile:po:view::allow',
    'tile:reconciliation:view::allow',
    'tile:search_coa:view::allow'
  )
 WHERE r.department_id = 'accounting';

INSERT INTO perm.role_permissions (role_id, role_kind, permission_id, granted_by)
SELECT r.id, r.kind, p.id, 'org-rebuild-2026-07-29'
  FROM perm.roles r
  JOIN perm.permissions p ON p.id IN (
    'finance:expense:accounting_approve::allow',
    'finance:gl:confirm::allow',
    'finance:po:approve::allow',
    'finance:po:reject::allow',
    'finance:pr:approve::allow',
    'stage:accounting_approval:act::allow',
    'stage:accounting_supervision:act::allow'
  )
 WHERE r.department_id = 'accounting' AND r.rank <= 4;

INSERT INTO perm.role_permissions (role_id, role_kind, permission_id, granted_by)
SELECT r.id, r.kind, p.id, 'org-rebuild-2026-07-29'
  FROM perm.roles r
  JOIN perm.permissions p ON p.id IN (
    'finance:expense:settlement_post::allow',
    'finance:gl:post::allow',
    'stage:accounting_authorization:act::allow',
    'stage:settlement:act::allow'
  )
 WHERE r.id = 'accounting_manager';

INSERT INTO perm.role_permissions (role_id, role_kind, permission_id, granted_by)
SELECT r.id, r.kind, p.id, 'org-rebuild-2026-07-29'
  FROM perm.roles r
  JOIN perm.permissions p ON p.id IN (
    'finance:cashflow:read::allow',
    'finance:expense:claim::allow',
    'finance:expense:pay::allow',
    'finance:expense:view_all::allow',
    'finance:po:attach_payslip::allow',
    'stage:payment:act::allow',
    'tile:customers:view::allow',
    'tile:dash_finance:view::allow',
    'tile:po:view::allow'
  )
 WHERE r.department_id = 'finance';

INSERT INTO perm.role_permissions (role_id, role_kind, permission_id, granted_by)
SELECT r.id, r.kind, p.id, 'org-rebuild-2026-07-29'
  FROM perm.roles r
  JOIN perm.permissions p ON p.id IN (
    'finance:customer:manage::allow',
    'finance:expense:approve::allow',
    'finance:expense:settle::allow',
    'finance:po:settle::allow',
    'stage:disbursement_authorization:act::allow'
  )
 WHERE r.department_id = 'finance' AND r.rank <= 4;

INSERT INTO perm.role_permissions (role_id, role_kind, permission_id, granted_by)
SELECT r.id, r.kind, p.id, 'org-rebuild-2026-07-29'
  FROM perm.roles r
  JOIN perm.permissions p ON p.id IN (
    'finance:budget:view::allow',
    'finance:po:approve::allow',
    'finance:po:reject::allow',
    'finance:pr:approve::allow',
    'stage:final_authorization:act::allow'
  )
 WHERE r.department_id = 'finance' AND r.rank <= 3;

INSERT INTO perm.role_permissions (role_id, role_kind, permission_id, granted_by)
SELECT r.id, r.kind, p.id, 'org-rebuild-2026-07-29'
  FROM perm.roles r
  JOIN perm.permissions p ON p.id IN (
    'finance:expense:executive_approve::allow',
    'finance:expense:override::allow',
    'finance:expense:override_approve::allow',
    'finance:pr:override_approve::allow',
    'finance:report:executive::allow',
    'stage:cfo_authorization:act::allow',
    'stage:executive_approval:act::allow',
    'stage:po_cfo:act::allow',
    'tile:cockpit:view::allow',
    'tile:executive:view::allow',
    'tile:summary:view::allow'
  )
 WHERE r.id = 'cfo';

INSERT INTO perm.role_permissions (role_id, role_kind, permission_id, granted_by)
SELECT r.id, r.kind, p.id, 'org-rebuild-2026-07-29'
  FROM perm.roles r
  JOIN perm.permissions p ON p.id IN (
    'finance:budget:view::allow',
    'finance:cashflow:read::allow',
    'finance:expense:approve::allow',
    'finance:expense:executive_approve::allow',
    'finance:expense:override::allow',
    'finance:expense:override_approve::allow',
    'finance:expense:view_all::allow',
    'finance:pr:override_approve::allow',
    'finance:report:executive::allow',
    'rbac:audit:view::allow',
    'rbac:matrix:view::allow',
    'stage:ceo_authorization:act::allow',
    'stage:executive_approval:act::allow',
    'tile:audit:view::allow',
    'tile:cockpit:view::allow',
    'tile:dash_exec:view::allow',
    'tile:executive:view::allow',
    'tile:summary:view::allow'
  )
 WHERE r.id = 'ceo';

INSERT INTO perm.department_permissions (department_id, permission_id, granted_by)
SELECT d.id, p.id, 'org-rebuild-2026-07-29'
  FROM perm.departments d
  JOIN perm.permissions p ON p.id IN (
    'system:authenticated:view::allow',
    'user:directory:read::allow',
    'org:dept:read::allow',
    'org:tree:view::allow',
    'tile:directory:view::allow',
    'tile:inbox:view::allow',
    'tile:org_chart:view::allow'
  );

INSERT INTO perm.department_permissions (department_id, permission_id, granted_by)
SELECT 'it', p.id, 'org-rebuild-2026-07-29'
  FROM perm.permissions p
 WHERE p.id IN ('tile:dash_it:view::allow', 'tile:hook:view::allow', 'tile:settings:view::allow');

INSERT INTO perm.department_permissions (department_id, permission_id, granted_by)
SELECT 'hr', p.id, 'org-rebuild-2026-07-29'
  FROM perm.permissions p
 WHERE p.id IN (
   'hr:employee:list::allow', 'hr:employee:read::allow', 'hr:leave:read::allow',
   'hr:leave:submit::allow', 'hr:quota:read::allow', 'tile:hr:view::allow',
   'tile:hr_employees:view::allow', 'tile:hr_leave:view::allow', 'tile:me_leave:view::allow'
 );

INSERT INTO perm.department_permissions (department_id, permission_id, granted_by)
SELECT 'accounting', p.id, 'org-rebuild-2026-07-29'
  FROM perm.permissions p
 WHERE p.id IN (
   'finance:expense:review::allow', 'finance:expense:view_all::allow',
   'finance:gl:view::allow', 'finance:ledger:view::allow', 'tile:ledger:view::allow',
   'tile:po:view::allow', 'tile:reconciliation:view::allow', 'tile:search_coa:view::allow'
 );

INSERT INTO perm.department_permissions (department_id, permission_id, granted_by)
SELECT 'finance', p.id, 'org-rebuild-2026-07-29'
  FROM perm.permissions p
 WHERE p.id IN (
   'finance:cashflow:read::allow', 'finance:expense:view_all::allow',
   'tile:customers:view::allow', 'tile:dash_finance:view::allow', 'tile:po:view::allow'
 );

INSERT INTO perm.department_permissions (department_id, permission_id, granted_by)
SELECT 'executive', p.id, 'org-rebuild-2026-07-29'
  FROM perm.permissions p
 WHERE p.id IN (
   'finance:budget:view::allow', 'finance:cashflow:read::allow',
   'finance:expense:view_all::allow', 'finance:report:executive::allow',
   'tile:cockpit:view::allow', 'tile:dash_exec:view::allow',
   'tile:executive:view::allow', 'tile:summary:view::allow'
 );

INSERT INTO perm.audit (kind, actor, target)
VALUES (
  'org.structure.rebuild',
  'migration',
  jsonb_build_object(
    'activeUsers', 0,
    'archivedUsers', (SELECT count(*) FROM users),
    'departments', (SELECT count(*) FROM perm.departments),
    'roles', (SELECT count(*) FROM perm.roles)
  )
);

DO $do$
BEGIN
  IF (SELECT count(*) FROM users WHERE is_active) <> 0 THEN
    RAISE EXCEPTION 'Expected zero active users';
  END IF;
  IF (SELECT count(*) FROM perm.departments) <> 5 THEN
    RAISE EXCEPTION 'Expected five departments';
  END IF;
  IF (SELECT count(*) FROM perm.roles) <> 14 THEN
    RAISE EXCEPTION 'Expected fourteen roles';
  END IF;
  IF EXISTS (SELECT 1 FROM perm.roles WHERE kind <> 'hierarchy' OR department_id IS NULL) THEN
    RAISE EXCEPTION 'Every rebuilt role must be a department-owned hierarchy role';
  END IF;
  IF EXISTS (SELECT 1 FROM perm.user_roles UNION ALL SELECT 1 FROM perm.user_departments) THEN
    RAISE EXCEPTION 'Expected zero user access bindings';
  END IF;
  IF EXISTS (
    SELECT 1
      FROM perm.roles r
     WHERE NOT EXISTS (
       SELECT 1 FROM perm.role_permissions rp
        WHERE rp.role_id = r.id AND rp.permission_id = 'system:authenticated:view::allow'
     )
  ) THEN
    RAISE EXCEPTION 'Every rebuilt role must have the authenticated baseline';
  END IF;
END
$do$;

COMMIT;
