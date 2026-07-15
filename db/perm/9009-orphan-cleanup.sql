-- db/perm/9009-orphan-cleanup.sql
--
-- Wire up orphan tile gates + orphan feature perms that the application code
-- already assumes (so a user actually CAN exercise the feature), then drop
-- legacy perms that no route, no RBAC matrix, no policy stage, no API guard
-- references anywhere in lib/ or app/src.
--
-- Section A: REMOVE (no consumer)
-- Section B: WIRE  (grant to role the code assumes)
--
-- Idempotent: re-runs are safe. Backed up to /tmp/perm-pre-9009.sql before applying.

BEGIN;

-- =========================================================================
-- SECTION B — Wire orphan role_permissions
-- =========================================================================

-- The matrix below mirrors what was agreed in the Orphan Cleanup plan.
-- Roles: officer::5, hr::5, it::2, account_officer::5, account_supervisor::4,
--        supervisor::4, sales_rep::3, manager::3, hr_manager::3,
--        accounting_manager::3, sales_supervisor::2, finance::2,
--        admin::2, cfo::2, ceo::1

INSERT INTO perm.role_permissions (role_id, permission_id, granted_at, granted_by)
SELECT r.id, p.id, now(), 'system-9009'
FROM (VALUES
  -- (role_id, permission_id)
  -- tile gates
  ('finance::2',            'tile:po:view::allow'),
  ('manager::3',            'tile:po:view::allow'),
  ('accounting_manager::3', 'tile:po:view::allow'),
  ('admin::2',              'tile:po:view::allow'),
  ('cfo::2',                'tile:po:view::allow'),
  ('ceo::1',                'tile:po:view::allow'),

  ('sales_rep::3',          'tile:sales:view::allow'),
  ('sales_supervisor::2',   'tile:sales:view::allow'),
  ('admin::2',              'tile:sales:view::allow'),

  ('sales_rep::3',          'tile:customers:view::allow'),
  ('sales_supervisor::2',   'tile:customers:view::allow'),
  ('admin::2',              'tile:customers:view::allow'),

  ('account_officer::5',    'tile:search_coa:view::allow'),
  ('account_supervisor::4', 'tile:search_coa:view::allow'),
  ('accounting_manager::3', 'tile:search_coa:view::allow'),
  ('finance::2',            'tile:search_coa:view::allow'),
  ('admin::2',              'tile:search_coa:view::allow'),
  ('cfo::2',                'tile:search_coa:view::allow'),
  ('ceo::1',                'tile:search_coa:view::allow'),

  ('it::2',                 'tile:policy:view::allow'),
  ('admin::2',              'tile:policy:view::allow'),

  ('hr_manager::3',         'tile:departments:view::allow'),
  ('admin::2',              'tile:departments:view::allow'),

  -- finance.expense
  ('account_officer::5',    'finance:expense:view_all::allow'),
  ('account_supervisor::4', 'finance:expense:view_all::allow'),
  ('accounting_manager::3', 'finance:expense:view_all::allow'),
  ('finance::2',            'finance:expense:view_all::allow'),
  ('admin::2',              'finance:expense:view_all::allow'),
  ('cfo::2',                'finance:expense:view_all::allow'),
  ('ceo::1',                'finance:expense:view_all::allow'),

  ('officer::5',            'finance:expense:update::allow'),
  ('hr::5',                 'finance:expense:update::allow'),
  ('it::2',                 'finance:expense:update::allow'),
  ('account_officer::5',    'finance:expense:update::allow'),
  ('account_supervisor::4', 'finance:expense:update::allow'),
  ('supervisor::4',         'finance:expense:update::allow'),
  ('sales_rep::3',          'finance:expense:update::allow'),
  ('manager::3',            'finance:expense:update::allow'),
  ('hr_manager::3',         'finance:expense:update::allow'),
  ('accounting_manager::3', 'finance:expense:update::allow'),
  ('sales_supervisor::2',   'finance:expense:update::allow'),
  ('finance::2',            'finance:expense:update::allow'),
  ('admin::2',              'finance:expense:update::allow'),
  ('cfo::2',                'finance:expense:update::allow'),
  ('ceo::1',                'finance:expense:update::allow'),

  ('manager::3',            'finance:expense:review::allow'),
  ('finance::2',            'finance:expense:review::allow'),

  -- finance.po
  ('finance::2',            'finance:po:attach_payslip::allow'),
  ('admin::2',              'finance:po:attach_payslip::allow'),

  ('finance::2',            'finance:po:approve::allow'),
  ('manager::3',            'finance:po:approve::allow'),
  ('admin::2',              'finance:po:approve::allow'),

  ('finance::2',            'finance:po:reject::allow'),
  ('manager::3',            'finance:po:reject::allow'),
  ('admin::2',              'finance:po:reject::allow'),

  ('finance::2',            'finance:po:settle::allow'),
  ('admin::2',              'finance:po:settle::allow'),

  -- finance.pr.override (CFO/CEO override pipeline)
  ('cfo::2',                'finance:pr:override_approve::allow'),
  ('ceo::1',                'finance:pr:override_approve::allow'),

  -- stage.final_authorization (alias for po_pending, used by Manager)
  ('manager::3',            'stage:final_authorization:act::allow'),

  -- user.profile / dept / manager / subtree
  ('hr_manager::3',         'user:profile:create::allow'),
  ('admin::2',              'user:profile:create::allow'),

  ('hr_manager::3',         'user:profile:update::allow'),
  ('admin::2',              'user:profile:update::allow'),

  ('admin::2',              'user:profile:delete::allow'),

  ('hr_manager::3',         'user:profile:deactivate::allow'),
  ('admin::2',              'user:profile:deactivate::allow'),

  ('hr_manager::3',         'user:manager:set::allow'),
  ('admin::2',              'user:manager:set::allow'),

  ('hr_manager::3',         'user:dept:edit::allow'),
  ('admin::2',              'user:dept:edit::allow'),

  ('manager::3',            'user:subtree:edit::allow'),
  ('hr_manager::3',         'user:subtree:edit::allow'),
  ('admin::2',              'user:subtree:edit::allow'),

  -- org.dept_role (HR + admin only)
  ('hr_manager::3',         'org:dept_role:assign::allow'),
  ('admin::2',              'org:dept_role:assign::allow'),

  ('hr_manager::3',         'org:dept_role:revoke::allow'),
  ('admin::2',              'org:dept_role:revoke::allow'),

  ('hr_manager::3',         'org:dept_role:list::allow'),
  ('admin::2',              'org:dept_role:list::allow')
) AS m(role_id, permission_id)
JOIN perm.roles r ON r.id = m.role_id
JOIN perm.permissions p ON p.id = m.permission_id
ON CONFLICT (role_id, permission_id) DO NOTHING;

-- =========================================================================
-- SECTION A — Remove orphan permission rows
-- =========================================================================

-- Delete dependent role_permissions first (idempotent if already absent).
DELETE FROM perm.role_permissions WHERE permission_id IN (
  'tile:hub:view::allow',
  'tile:hook_view:view::allow',
  'tile:hook_inbox:view::allow',
  'tile:team_manage:view::allow',
  'tile:permissions:view::allow',
  'tile:dash_hr:view::allow',
  'tile:dash_reviewer:view::allow',
  'tile:ops_overview:view::allow',
  'finance:client_contracts:view::allow',
  'finance:core_operations:view::allow',
  'finance:project_tasks:view::allow',
  'finance:reconciliation:view::allow',
  'finance:expense:delete::allow',
  'finance:expense:reject::allow',
  'finance:expense:disburse::allow',
  'finance:pr:delete::allow',
  'finance:pr:reject::allow',
  'org:hook:read::allow'
);

-- Delete user_permissions rows that reference the orphan perms (if any).
DELETE FROM perm.user_permissions WHERE permission_id IN (
  'tile:hub:view::allow',
  'tile:hook_view:view::allow',
  'tile:hook_inbox:view::allow',
  'tile:team_manage:view::allow',
  'tile:permissions:view::allow',
  'tile:dash_hr:view::allow',
  'tile:dash_reviewer:view::allow',
  'tile:ops_overview:view::allow',
  'finance:client_contracts:view::allow',
  'finance:core_operations:view::allow',
  'finance:project_tasks:view::allow',
  'finance:reconciliation:view::allow',
  'finance:expense:delete::allow',
  'finance:expense:reject::allow',
  'finance:expense:disburse::allow',
  'finance:pr:delete::allow',
  'finance:pr:reject::allow',
  'org:hook:read::allow'
);

-- Finally delete the permission rows themselves.
DELETE FROM perm.permissions WHERE id IN (
  'tile:hub:view::allow',
  'tile:hook_view:view::allow',
  'tile:hook_inbox:view::allow',
  'tile:team_manage:view::allow',
  'tile:permissions:view::allow',
  'tile:dash_hr:view::allow',
  'tile:dash_reviewer:view::allow',
  'tile:ops_overview:view::allow',
  'finance:client_contracts:view::allow',
  'finance:core_operations:view::allow',
  'finance:project_tasks:view::allow',
  'finance:reconciliation:view::allow',
  'finance:expense:delete::allow',
  'finance:expense:reject::allow',
  'finance:expense:disburse::allow',
  'finance:pr:delete::allow',
  'finance:pr:reject::allow',
  'org:hook:read::allow'
);

COMMIT;
