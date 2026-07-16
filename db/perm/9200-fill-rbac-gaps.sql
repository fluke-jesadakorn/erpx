-- 9200-fill-rbac-gaps.sql
-- Closes 12 orphan-permission gaps and adds dept markers + dept-head roles
-- for Sales and Law, so the RBAC matrix covers every persona cleanly.
--
-- Adds:
--   1. Permissions    5 new
--   2. Dept-head roles 2 (dept-law, dept-sales)
--   3. Role grants   every persona gets hr:leave:submit + tiles, orphan perms covered
--   4. LW user rebind hr -> law
--
-- Idempotent. Safe to re-run.

BEGIN;

-- ════════════════════════════════════════════════════════════════════════════
-- 1. PERMISSIONS
-- ════════════════════════════════════════════════════════════════════════════

INSERT INTO perm.permissions (id, description) VALUES
  ('user:dept:sales::allow',               'Department membership: Sales'),
  ('user:dept:law::allow',                 'Department membership: Law'),
  ('finance:client_contracts:view::allow', 'View client contracts'),
  ('finance:core_operations:view::allow',  'View core operations'),
  ('finance:project_tasks:view::allow',    'View project tasks')
ON CONFLICT (id) DO NOTHING;

-- ════════════════════════════════════════════════════════════════════════════
-- 2. DEPT-HEAD ROLES (parallels existing dept-development / dept-it / …)
-- ════════════════════════════════════════════════════════════════════════════

INSERT INTO perm.roles (id, display_name, description, is_system, sort_order) VALUES
  ('dept-law',   'Legal Dept Head', 'Counsel / legal team lead',    true, 7),
  ('dept-sales', 'Sales Dept Head', 'Sales team lead',              true, 8)
ON CONFLICT (id) DO NOTHING;

-- ════════════════════════════════════════════════════════════════════════════
-- 3. ROLE GRANTS — close every orphan perm + extend personas
-- ════════════════════════════════════════════════════════════════════════════

-- G1: hr:leave:submit -> EVERY persona (baseline bundle pattern)
INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT r.id, 'hr:leave:submit::allow', 'baseline'
  FROM perm.roles r
 WHERE split_part(r.id, '::', 1) NOT IN ('admin')
    OR r.id = 'admin::2'
ON CONFLICT DO NOTHING;
-- admin gets it too via bypass but explicit grant keeps the matrix honest
INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT 'admin::2', 'hr:leave:submit::allow', 'baseline'
ON CONFLICT DO NOTHING;

-- G2: law:admin:retention_set -> counsel
INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT 'counsel::3', 'law:admin:retention_set::allow', 'role'
ON CONFLICT DO NOTHING;

-- G3 (paralegal edits): law:contract:update -> law::5
INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT 'law::5', 'law:contract:update::allow', 'role'
ON CONFLICT DO NOTHING;

-- Counsel expansion per plan: also gets finance:budget:view
INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT 'counsel::3', 'finance:budget:view::allow', 'role'
ON CONFLICT DO NOTHING;

-- G4: tile:customers + tile:sales visibility for accounting & finance flows
INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT r.id, p.id, 'role'
  FROM perm.roles r
  JOIN (VALUES
    ('accounting_manager::3', 'tile:customers:view::allow'),
    ('accounting_manager::3', 'tile:sales:view::allow'),
    ('finance::2',            'tile:customers:view::allow'),
    ('finance::2',            'tile:sales:view::allow'),
    ('admin::2',              'tile:customers:view::allow'),
    ('admin::2',              'tile:sales:view::allow')
  ) AS m(role_id, perm_id) ON m.role_id = r.id
  JOIN perm.permissions p ON p.id = m.perm_id
ON CONFLICT DO NOTHING;

-- G12: customer:manage -> sales + finance + exec + admin
INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT r.id, 'customer:manage::allow', 'role'
  FROM perm.roles r
 WHERE split_part(r.id, '::', 1) IN (
   'sales_rep','sales_supervisor','accounting_manager','finance','cfo','ceo','admin'
 )
ON CONFLICT DO NOTHING;

-- G11: finance executive views (client_contracts / core_operations / project_tasks)
INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT r.id, p.id, 'role'
  FROM perm.roles r
  CROSS JOIN perm.permissions p
 WHERE split_part(r.id, '::', 1) IN ('cfo','ceo','admin','finance')
   AND p.id IN (
     'finance:client_contracts:view::allow',
     'finance:core_operations:view::allow',
     'finance:project_tasks:view::allow'
   )
ON CONFLICT DO NOTHING;

-- G10: org:hook:read + hook:event:view for admin (already on it::2 via seed)
INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT 'admin::2', p.id, 'role'
  FROM perm.permissions p
 WHERE p.id IN ('org:hook:read::allow', 'hook:event:view::allow', 'hook:event:replay::allow')
ON CONFLICT DO NOTHING;

-- Sales + Law get their tile gates (already on reps via seed; ensure supervisors also)
INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT r.id, p.id, 'role'
  FROM perm.roles r
  JOIN (VALUES
    ('sales_rep::3',        'tile:customers:view::allow'),
    ('sales_rep::3',        'tile:sales:view::allow'),
    ('sales_supervisor::2', 'tile:customers:view::allow'),
    ('sales_supervisor::2', 'tile:sales:view::allow'),
    ('law::5',              'tile:law_upload:view::allow')
  ) AS m(role_id, perm_id) ON m.role_id = r.id
  JOIN perm.permissions p ON p.id = m.perm_id
ON CONFLICT DO NOTHING;

-- CEO/Admin get the HR/Law tile gates for oversight
INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT r.id, p.id, 'role'
  FROM perm.roles r
  CROSS JOIN perm.permissions p
 WHERE split_part(r.id, '::', 1) IN ('ceo','admin')
   AND p.id IN ('tile:hr:view::allow','tile:hr_employees:view::allow','tile:hr_leave:view::allow',
                'tile:law:view::allow','tile:law_admin:view::allow','tile:law_upload:view::allow')
ON CONFLICT DO NOTHING;

-- CFO gets HR/Law oversight too
INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT r.id, p.id, 'role'
  FROM perm.roles r
  CROSS JOIN perm.permissions p
 WHERE split_part(r.id, '::', 1) = 'cfo'
   AND p.id IN ('tile:hr:view::allow','tile:hr_employees:view::allow','tile:hr_leave:view::allow',
                'tile:law:view::allow')
ON CONFLICT DO NOTHING;

-- ════════════════════════════════════════════════════════════════════════════
-- 4. LW USERS REBIND hr -> law
-- ════════════════════════════════════════════════════════════════════════════

-- Revoke the wrong dept marker (LW001, LW002)
UPDATE perm.user_permissions
   SET revoked_at = NOW(), revoked_by = 1, reason = 'rebinding to dept-law (9200)'
 WHERE permission_id = 'user:dept:hr::allow'
   AND revoked_at IS NULL
   AND user_id IN (SELECT id FROM folio.users WHERE employee_code IN ('LW001','LW002'));

-- Grant the correct dept marker
INSERT INTO perm.user_permissions (user_id, permission_id, granted_by, reason)
SELECT u.id, 'user:dept:law::allow', 1, 'seed 9200'
  FROM folio.users u
 WHERE u.employee_code IN ('LW001','LW002')
   AND NOT EXISTS (
     SELECT 1 FROM perm.user_permissions up
      WHERE up.user_id = u.id
        AND up.permission_id = 'user:dept:law::allow'
        AND up.revoked_at IS NULL
   );

-- Sales-role users currently under dept-marketing — give them dept-sales too (additive)
INSERT INTO perm.user_permissions (user_id, permission_id, granted_by, reason)
SELECT u.id, 'user:dept:sales::allow', 1, 'seed 9200 (sales persona)'
  FROM folio.users u
 WHERE u.employee_code IN ('EMP007','EMP010','EMP020','EMP021','EMP025')
   AND NOT EXISTS (
     SELECT 1 FROM perm.user_permissions up
      WHERE up.user_id = u.id
        AND up.permission_id = 'user:dept:sales::allow'
        AND up.revoked_at IS NULL
   );

COMMIT;