-- 9001-seed-new-grammar.sql
-- Single consolidated seed rebuilt for the :: string grammar.
--
-- Roles:           id = '<name>::<level>'   (1=highest authority, 10=lowest)
-- Permissions:     id = '<d>:<s>:<v>[:<q>]::<effect>'
-- Department:      user:dept:<id>::allow granted to user via perm.user_permissions
-- Tile gate:       perm.tiles.view_perm_id (e.g. 'tile:expense:view::allow')

BEGIN;

-- ════════════════════════════════════════════════════════════════════════════
-- 1. ROLES
-- ════════════════════════════════════════════════════════════════════════════

INSERT INTO perm.roles (id, display_name, description, is_system, sort_order) VALUES
  -- Persona roles with inline level.
  ('officer::5',           'Officer',             'Front-line operational staff',     true,  100),
  ('hr::5',                'HR Officer',          'Human-resources front-desk',       true,  101),
  ('it::2',                'IT Officer',          'Information technology',           true,  102),
  ('account_officer::5',   'Accounting Officer',  'Front-line accounting',            true,  103),
  ('account_supervisor::4','Accounting Supervisor','Accounting team lead',            true,  104),
  ('supervisor::4',        'Supervisor',          'Direct team supervisor',           true,  105),
  ('sales_rep::3',         'Sales Rep',           'Sales frontline',                  true,  106),
  ('manager::3',           'Manager',             'Department-level manager',         true,  110),
  ('hr_manager::3',        'HR Manager',          'HR team manager',                  true,  111),
  ('accounting_manager::3','Accounting Manager',  'Accounting team manager',          true,  112),
  ('sales_supervisor::2',  'Sales Supervisor',    'Sales team supervisor',            true,  113),
  ('finance::2',           'Finance Lead',        'Disbursement lead',                true,  120),
  ('admin::2',             'Admin',               'System administrator',             true,  130),
  ('cfo::2',               'CFO',                 'Chief Financial Officer',          true,  131),
  ('ceo::1',               'CEO',                 'Chief Executive Officer',          true,  132)
ON CONFLICT (id) DO UPDATE SET
  display_name = EXCLUDED.display_name,
  description = EXCLUDED.description,
  is_system = EXCLUDED.is_system,
  sort_order = EXCLUDED.sort_order;

-- ════════════════════════════════════════════════════════════════════════════
-- 2. PERMISSIONS (catalog — full :: string format)
-- ════════════════════════════════════════════════════════════════════════════

INSERT INTO perm.permissions (id, description)
SELECT id, NULL FROM (VALUES
  -- admin / rbac
  ('admin:system:bypass::allow'),
  ('rbac:role:read::allow'),
  ('rbac:role:assign::allow'),
  ('rbac:matrix:view::allow'),
  ('rbac:matrix:edit::allow'),
  ('rbac:audit:view::allow'),
  -- user
  ('user:directory:read::allow'),
  ('user:profile:create::allow'),
  ('user:profile:update::allow'),
  ('user:profile:delete::allow'),
  ('user:profile:deactivate::allow'),
  ('user:role:assign::allow'),
  ('user:manager:set::allow'),
  ('user:dept:edit::allow'),
  ('user:subtree:edit::allow'),
  -- org
  ('org:tree:view::allow'),
  ('org:dept:read::allow'),
  ('org:dept:assign_head::allow'),
  ('org:dept_role:assign::allow'),
  ('org:dept_role:revoke::allow'),
  ('org:dept_role:list::allow'),
  ('org:auto_wire:propose::allow'),
  ('org:auto_wire:apply::allow'),
  ('org:hook:read::allow'),
  -- finance:expense
  ('finance:expense:view_own::allow'),
  ('finance:expense:view_all::allow'),
  ('finance:expense:create::allow'),
  ('finance:expense:update::allow'),
  ('finance:expense:delete::allow'),
  ('finance:expense:review::allow'),
  ('finance:expense:approve::allow'),
  ('finance:expense:reject::allow'),
  ('finance:expense:settle::allow'),
  ('finance:expense:disburse::allow'),
  ('finance:expense:override::allow'),
  ('finance:expense:override_approve::allow'),
  ('finance:expense:gl_confirm::allow'),
  -- finance:pr
  ('finance:pr:create::allow'),
  ('finance:pr:update::allow'),
  ('finance:pr:delete::allow'),
  ('finance:pr:approve::allow'),
  ('finance:pr:reject::allow'),
  ('finance:pr:override_approve::allow'),
  -- finance:po
  ('finance:po:approve::allow'),
  ('finance:po:reject::allow'),
  ('finance:po:attach_payslip::allow'),
  ('finance:po:settle::allow'),
  -- finance:other
  ('finance:ledger:view::allow'),
  ('finance:reconciliation:view::allow'),
  ('finance:report:executive::allow'),
  ('finance:budget:view::allow'),
  ('finance:client_contracts:view::allow'),
  ('finance:core_operations:view::allow'),
  ('finance:project_tasks:view::allow'),
  -- stage:expense
  ('stage:submission:act::allow'),
  ('stage:dept_verification:act::allow'),
  ('stage:dept_authorization:act::allow'),
  ('stage:accounting_verification:act::allow'),
  ('stage:accounting_supervision:act::allow'),
  ('stage:accounting_authorization:act::allow'),
  ('stage:disbursement_authorization:act::allow'),
  ('stage:cfo_authorization:act::allow'),
  ('stage:ceo_authorization:act::allow'),
  ('stage:gl_confirmed:act::allow'),
  ('stage:final_authorization:act::allow'),
  -- stage:po
  ('stage:po_pending:act::allow'),
  ('stage:po_cfo:act::allow'),
  -- stage:sales
  ('stage:so_draft:act::allow'),
  ('stage:so_sales_review:act::allow'),
  ('stage:so_credit_check:act::allow'),
  ('stage:so_invoiced:act::allow'),
  ('stage:so_paid:act::allow'),
  -- tile:view
  ('tile:inbox:view::allow'),
  ('tile:expense:view::allow'),
  ('tile:pr:view::allow'),
  ('tile:po:view::allow'),
  ('tile:sales:view::allow'),
  ('tile:customers:view::allow'),
  ('tile:search_coa:view::allow'),
  ('tile:reconciliation:view::allow'),
  ('tile:team_manage:view::allow'),
  ('tile:cockpit:view::allow'),
  ('tile:summary:view::allow'),
  ('tile:ledger:view::allow'),
  ('tile:policy:view::allow'),
  ('tile:settings:view::allow'),
  ('tile:org_chart:view::allow'),
  ('tile:roles:view::allow'),
  ('tile:tile_gates:view::allow'),
  ('tile:directory:view::allow'),
  ('tile:audit:view::allow'),
  ('tile:departments:view::allow'),
  ('tile:access_requests:view::allow'),
  ('tile:hook:view::allow'),
  ('tile:hook_view:view::allow'),
  ('tile:hook_inbox:view::allow'),
  ('tile:permissions:view::allow'),
  ('tile:hub:view::allow'),
  ('tile:ops_overview:view::allow'),
  ('tile:my_prs:view::allow'),
  ('tile:dash_am:view::allow'),
  ('tile:dash_exec:view::allow'),
  ('tile:dash_finance:view::allow'),
  ('tile:dash_hr:view::allow'),
  ('tile:dash_it:view::allow'),
  ('tile:dash_manager:view::allow'),
  ('tile:dash_reviewer:view::allow'),
  ('tile:dash_staff:view::allow'),
  -- hook
  ('hook:event:view::allow'),
  ('hook:event:replay::allow'),
  -- ai
  ('ai:provider:read::allow'),
  ('ai:provider:create::allow'),
  ('ai:provider:update::allow'),
  ('ai:provider:delete::allow'),
  ('ai:provider:test::allow'),
  ('ai:model:read::allow'),
  ('ai:model:create::allow'),
  ('ai:model:update::allow'),
  ('ai:model:delete::allow'),
  ('ai:staff:read::allow'),
  ('ai:staff:create::allow'),
  ('ai:staff:update::allow'),
  ('ai:staff:delete::allow'),
  ('ai:staff:invoke::allow'),
  ('ai:assignment:read::allow'),
  ('ai:assignment:create::allow'),
  ('ai:assignment:delete::allow'),
  ('ai:invocation:view::allow'),
  ('ai:section_health:view::allow'),
  -- access_request
  ('access_request:request:list::allow'),
  ('access_request:request:resolve::allow'),
  -- system (global authenticated access — every signed-in user)
  ('system:authenticated:view::allow')
) AS v(id)
ON CONFLICT (id) DO NOTHING;

-- ════════════════════════════════════════════════════════════════════════════
-- 3. ROLE GRANTS (curated bundles)
-- ════════════════════════════════════════════════════════════════════════════

-- Baseline submit-own bundle: every persona gets expense+pr create, view-own, plus inbox tile,
-- plus the global `system:authenticated:view` so basic access is universal.
INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT r.id, p.id, 'baseline'
  FROM perm.roles r
 CROSS JOIN perm.permissions p
 WHERE p.id IN (
   'finance:expense:view_own::allow',
   'finance:expense:create::allow',
   'finance:pr:create::allow',
   'finance:pr:update::allow',
   'tile:inbox::allow',
   'tile:inbox:view::allow',
   'tile:expense:view::allow',
   'tile:pr:view::allow',
   'tile:my_prs:view::allow',
   'system:authenticated:view::allow'
 )
ON CONFLICT DO NOTHING;

-- Officer-class roles see own dashboard + expenses
INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT r.id, 'tile:dash_staff:view::allow', 'baseline'
  FROM perm.roles r
 WHERE split_part(r.id, '::', 1) IN ('officer','hr','account_officer','supervisor','sales_rep')
ON CONFLICT DO NOTHING;

-- Supervisor + manager get stage perms
INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT r.id, p.id, 'role'
  FROM perm.roles r
  JOIN (VALUES
    ('supervisor',         'stage:dept_verification:act::allow'),
    ('supervisor',         'stage:submission:act::allow'),
    ('manager',            'stage:dept_authorization:act::allow'),
    ('manager',            'stage:dept_verification:act::allow'),
    ('account_officer',    'stage:accounting_verification:act::allow'),
    ('account_supervisor', 'stage:accounting_supervision:act::allow'),
    ('accounting_manager', 'stage:accounting_authorization:act::allow'),
    ('finance',            'stage:disbursement_authorization:act::allow'),
    ('finance',            'stage:gl_confirmed:act::allow'),
    ('finance',            'finance:expense:settle::allow'),
    ('finance',            'finance:expense:approve::allow'),
    ('finance',            'finance:expense:gl_confirm::allow'),
    ('finance',            'tile:dash_finance:view::allow'),
    ('cfo',                'stage:cfo_authorization:act::allow'),
    ('cfo',                'finance:expense:override::allow'),
    ('cfo',                'finance:expense:override_approve::allow'),
    ('cfo',                'tile:dash_finance:view::allow'),
    ('ceo',                'stage:ceo_authorization:act::allow'),
    ('ceo',                'finance:expense:override::allow'),
    ('ceo',                'finance:expense:override_approve::allow'),
    ('manager',            'tile:dash_manager:view::allow'),
    ('accounting_manager', 'tile:dash_am:view::allow'),
    ('accounting_manager', 'tile:ledger:view::allow'),
    ('finance',            'tile:ledger:view::allow'),
    ('cfo',                'tile:ledger:view::allow'),
    ('ceo',                'tile:ledger:view::allow'),
    ('manager',            'finance:expense:approve::allow'),
    ('manager',            'finance:pr:approve::allow'),
    ('manager',            'stage:po_pending:act::allow'),
    ('cfo',                'stage:po_cfo:act::allow'),
    ('sales_rep',          'stage:so_draft:act::allow'),
    ('sales_rep',          'stage:so_sales_review:act::allow'),
    ('sales_supervisor',   'stage:so_credit_check:act::allow'),
    ('sales_supervisor',   'stage:so_invoiced:act::allow'),
    ('finance',            'stage:so_paid:act::allow')
  ) AS m(name, perm_id)
   ON split_part(r.id, '::', 1) = m.name
  JOIN perm.permissions p ON p.id = m.perm_id
ON CONFLICT DO NOTHING;

-- HR roles: directory + role assignment + tile:directory + tile:hr
INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT r.id, p.id, 'role'
  FROM perm.roles r
  JOIN (VALUES
    ('hr',          'tile:directory:view::allow'),
    ('hr_manager',  'tile:directory:view::allow'),
    ('hr_manager',  'tile:hr:view::allow'),
    ('hr_manager',  'tile:access_requests:view::allow'),
    ('hr_manager',  'user:role:assign::allow'),
    ('hr_manager',  'access_request:request:list::allow'),
    ('hr_manager',  'access_request:request:resolve::allow'),
    ('hr_manager',  'org:dept:assign_head::allow'),
    ('hr_manager',  'org:auto_wire:propose::allow'),
    ('hr_manager',  'org:auto_wire:apply::allow')
  ) AS m(name, perm_id)
   ON split_part(r.id, '::', 1) = m.name
  JOIN perm.permissions p ON p.id = m.perm_id
ON CONFLICT DO NOTHING;

-- IT: full AI + hook access + dash_it
INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT r.id, p.id, 'role'
  FROM perm.roles r
 CROSS JOIN perm.permissions p
 WHERE split_part(r.id, '::', 1) = 'it'
   AND p.id LIKE 'ai:%::allow'
ON CONFLICT DO NOTHING;
INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT r.id, p.id, 'role'
  FROM perm.roles r
  JOIN (VALUES
    ('it', 'tile:settings:view::allow'),
    ('it', 'tile:audit:view::allow'),
    ('it', 'tile:hook_view:view::allow'),
    ('it', 'tile:hook_inbox:view::allow'),
    ('it', 'tile:dash_it:view::allow'),
    ('it', 'tile:org_chart:view::allow'),
    ('it', 'tile:roles:view::allow'),
    ('it', 'tile:tile_gates:view::allow'),
    ('it', 'tile:permissions:view::allow'),
    ('it', 'tile:hook:view::allow'),
    ('it', 'hook:event:view::allow'),
    ('it', 'hook:event:replay::allow'),
    ('it', 'rbac:matrix:view::allow'),
    ('it', 'rbac:matrix:edit::allow'),
    ('it', 'rbac:audit:view::allow'),
    ('it', 'rbac:role:read::allow'),
    ('it', 'rbac:role:assign::allow'),
    ('it', 'org:tree:view::allow'),
    ('it', 'org:dept:read::allow'),
    ('it', 'user:directory:read::allow')
  ) AS m(name, perm_id)
   ON split_part(r.id, '::', 1) = m.name
  JOIN perm.permissions p ON p.id = m.perm_id
ON CONFLICT DO NOTHING;

-- admin: admin-bypass (grants everything)
INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT r.id, 'admin:system:bypass::allow', 'role'
  FROM perm.roles r
 WHERE split_part(r.id, '::', 1) = 'admin'
ON CONFLICT DO NOTHING;

-- cfo/ceo/admin get executive tiles
INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT r.id, p.id, 'role'
  FROM perm.roles r
 CROSS JOIN perm.permissions p
 WHERE split_part(r.id, '::', 1) IN ('cfo','ceo','admin')
   AND p.id IN (
     'tile:cockpit:view::allow',
     'tile:summary:view::allow',
     'tile:reconciliation:view::allow',
     'finance:report:executive::allow',
     'finance:budget:view::allow',
     'tile:dash_exec:view::allow',
     'finance:ledger:view::allow'
   )
ON CONFLICT DO NOTHING;

-- finance/cfo/ceo get dash_finance
INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT r.id, 'tile:dash_finance:view::allow', 'role'
  FROM perm.roles r
 WHERE split_part(r.id, '::', 1) IN ('finance','cfo','ceo','admin','accounting_manager')
ON CONFLICT DO NOTHING;

-- ════════════════════════════════════════════════════════════════════════════
-- 4. TILES (catalog — gate via view_perm_id)
-- ════════════════════════════════════════════════════════════════════════════

INSERT INTO perm.tiles (id, display_name, subtitle, icon, accent, group_name, href, sort_order, view_perm_id) VALUES
  ('inbox',           'Inbox',             'Approval queue',          '📥', 'sky',    'work',     '/inbox',            10,  'tile:inbox:view::allow'),
  ('expense',         'Expense',           'Submit / track claims',   '🧾', 'emerald','work',     '/expense',          20,  'tile:expense:view::allow'),
  ('pr',              'PR',                'Purchase requisitions',   '📝', 'violet', 'work',     '/inbox?scope=watching',             30, 'tile:pr:view::allow'),
  ('po',              'PO & Payslip',      'Purchase orders',         '📦', 'amber',  'work',     '/inbox?scope=all',                  40, 'tile:po:view::allow'),
  ('sales',           'Sales Orders',      'Sales orders',            '💼', 'cyan',   'work',     '/sales',            50,  'tile:sales:view::allow'),
  ('customers',       'Customers',         'Customer master',         '👤', 'cyan',   'work',     '/customers',        55,  'tile:customers:view::allow'),
  ('search-coa',      'Search',            'Chart of accounts',       '🔍', 'slate',  'work',     '/search-coa',       60,  'tile:search_coa:view::allow'),
  ('reconciliation',  'Reconciliation',    'Bank reconciliation',     '🔀', 'amber',  'finance',  '/reconciliation',   110, 'tile:reconciliation:view::allow'),
  ('ledger',          'General Ledger',    'GL entries',              '📚', 'slate',  'finance',  '/ledger',           120, 'tile:ledger:view::allow'),
  ('cockpit',         'Executive Cockpit', 'CFO/CEO view',            '🛡', 'rose',   'exec',     '/cockpit',          200, 'tile:cockpit:view::allow'),
  ('summary',         'Cross-Slice Summary','Cross-cut dashboard',    '📊', 'slate',  'exec',     '/summary',          210, 'tile:summary:view::allow'),
  ('policy',          'RBAC Policy',       'Stage matrix',            '🎯', 'violet', 'admin',    '/policy',           300, 'tile:policy:view::allow'),
  ('org-chart',       'Org Chart',         'Org tree',                '🌳', 'sky',    'admin',    '/org-chart',        310, 'tile:org_chart:view::allow'),
  ('roles',           'Roles',             'Persona catalog',         '🪪', 'violet', 'admin',    '/policy',           320,  'tile:roles:view::allow'),
  ('tile-gates',      'Tile Gates',        'Visibility gates',        '🚪', 'slate',  'admin',    '/tiles',            330, 'tile:tile_gates:view::allow'),
  ('directory',       'User Management',   'User directory',          '👥', 'sky',    'admin',    '/directory',        340, 'tile:directory:view::allow'),
  ('audit',           'Audit',             'Audit log',               '📜', 'slate',  'admin',    '/audit',            350, 'tile:audit:view::allow'),
  ('departments',     'Departments',       'Dept registry',           '🏢', 'sky',    'admin',    '/departments',      360, 'tile:departments:view::allow'),
  ('access-requests', 'Access Requests',   'Pending access',          '🔑', 'amber',  'admin',    '/access-requests',  370, 'tile:access_requests:view::allow'),
  ('settings',        'AI Settings',       'Provider/model catalog',  '⚙️', 'slate',  'admin',    '/settings',         380, 'tile:settings:view::allow'),
  ('hook',            'Hook Events',       'Webhook log',             '🪝', 'slate',  'admin',    '/hook',             390, 'tile:hook:view::allow')
ON CONFLICT (id) DO UPDATE SET
  display_name = EXCLUDED.display_name,
  subtitle = EXCLUDED.subtitle,
  icon = EXCLUDED.icon,
  accent = EXCLUDED.accent,
  group_name = EXCLUDED.group_name,
  href = EXCLUDED.href,
  sort_order = EXCLUDED.sort_order,
  view_perm_id = EXCLUDED.view_perm_id;

-- ════════════════════════════════════════════════════════════════════════════
-- 5. POLICIES (seed for the waybill oracle)
-- ════════════════════════════════════════════════════════════════════════════

INSERT INTO perm.policies (id, name, ast, description, enabled) VALUES
  ('canActOnWaybillStage', 'Can act on waybill stage',
   '{"kind":"and","rules":[]}'::jsonb,
   'Actor may act on the current stage of the waybill', true),
  ('recallWaybill',        'Recall a waybill',
   '{"kind":"and","rules":[]}'::jsonb,
   'Submitter can recall a waybill before final approval', true)
ON CONFLICT (id) DO NOTHING;

COMMIT;
