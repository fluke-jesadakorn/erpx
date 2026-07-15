-- Folio — seed perm.permissions (~80 ids in nested domain:subject:verb).
-- Generated from the existing rbac.modules grouped by group_name.

BEGIN;

-- rbac · matrix editor & admin
INSERT INTO perm.permissions (id, domain, subject, verb, description) VALUES
  ('rbac:role:read',             'rbac',   'role',     'read',   'View roles and their grants'),
  ('rbac:role:assign',           'rbac',   'role',     'assign', 'Assign roles to users'),
  ('rbac:matrix:view',           'rbac',   'matrix',   'view',   'View the permission matrix'),
  ('rbac:matrix:edit',           'rbac',   'matrix',   'edit',   'Edit the permission matrix'),
  ('rbac:audit:view',            'rbac',   'audit',    'view',   'View permission audit log'),

-- user · HR directory, dept, subtree
  ('user:directory:read',        'user',   'directory', 'read',   'Read user directory'),
  ('user:profile:create',        'user',   'profile',  'create', 'Create new users'),
  ('user:profile:update',        'user',   'profile',  'update', 'Edit users in their dept'),
  ('user:profile:delete',        'user',   'profile',  'delete', 'Delete (hard-remove) users'),
  ('user:profile:deactivate',    'user',   'profile',  'deactivate', 'Deactivate users'),
  ('user:role:assign',           'user',   'role',     'assign', 'Assign role to user'),
  ('user:manager:set',           'user',   'manager',  'set',    'Set reports-to manager'),
  ('user:dept:edit',             'user',   'dept',     'edit',   'Edit a user''s department'),
  ('user:subtree:edit',          'user',   'subtree',  'edit',   'Edit users in reporting subtree'),

-- org · org chart & departments
  ('org:tree:view',              'org',    'tree',      'view',     'View org chart'),
  ('org:dept:read',              'org',    'dept',      'read',     'Read departments'),
  ('org:dept:assign_head',       'org',    'dept',      'assign_head', 'Assign department head'),
  ('org:auto_wire:propose',      'org',    'auto_wire', 'propose',  'Propose auto-wired org tree'),
  ('org:auto_wire:apply',        'org',    'auto_wire', 'apply',    'Apply auto-wired org tree'),
  ('org:hook:read',              'org',    'hook',      'read',     'Read hook events'),

-- finance · expense / PR / PO / ledger
  ('finance:expense:view_own',   'finance', 'expense', 'view_own',  'View own expenses'),
  ('finance:expense:view_all',   'finance', 'expense', 'view_all',  'View all expenses'),
  ('finance:expense:create',     'finance', 'expense', 'create',    'Submit expense'),
  ('finance:expense:update',     'finance', 'expense', 'update',    'Edit expense (subject to ACL on submitter_id)'),
  ('finance:expense:delete',     'finance', 'expense', 'delete',    'Delete expense (subject to ACL)'),
  ('finance:expense:review',     'finance', 'expense', 'review',    'Accountant review (OCR → accountant_reviewed)'),
  ('finance:expense:approve',    'finance', 'expense', 'approve',   'Approve at any stage'),
  ('finance:expense:reject',     'finance', 'expense', 'reject',    'Reject expense'),
  ('finance:expense:settle',     'finance', 'expense', 'settle',    'Settle (post to GL)'),
  ('finance:expense:disburse',   'finance', 'expense', 'disburse',  'Finance disbursement (mark paid)'),
  ('finance:expense:override',   'finance', 'expense', 'override',  'CEO override on any stage'),
  ('finance:pr:create',          'finance', 'pr',      'create',    'Create purchase requisition'),
  ('finance:pr:update',          'finance', 'pr',      'update',    'Edit PR (subject to ACL on requester_id)'),
  ('finance:pr:delete',          'finance', 'pr',      'delete',    'Delete PR (subject to ACL)'),
  ('finance:pr:approve',         'finance', 'pr',      'approve',   'Approve PR at any stage'),
  ('finance:pr:reject',          'finance', 'pr',      'reject',    'Reject PR'),
  ('finance:po:approve',         'finance', 'po',      'approve',   'Approve PO'),
  ('finance:po:reject',          'finance', 'po',      'reject',    'Reject PO'),
  ('finance:po:attach_payslip',  'finance', 'po',      'attach_payslip', 'Attach payslip to PO'),
  ('finance:po:settle',          'finance', 'po',      'settle',    'Settle PO'),
  ('finance:ledger:view',        'finance', 'ledger',  'view',      'View GL / ledger'),
  ('finance:reconciliation:view','finance', 'reconciliation', 'view','View reconciliation dashboard'),
  ('finance:report:executive',   'finance', 'report',  'executive', 'View executive financial report'),
  ('finance:budget:view',        'finance', 'budget',  'view',      'View budget'),
  ('finance:client_contracts:view','finance', 'client_contracts', 'view', 'View client contracts'),
  ('finance:core_operations:view','finance', 'core_operations', 'view', 'View core operations dashboard'),
  ('finance:project_tasks:view',  'finance', 'project_tasks',  'view',  'View project tasks'),

-- stage · approval-chain gates
  ('stage:supervisor_review:act',         'stage', 'supervisor_review', 'act', 'Act on supervisor_review stage'),
  ('stage:manager_review:act',            'stage', 'manager_review',    'act', 'Act on manager_review stage'),
  ('stage:account_officer_review:act',    'stage', 'account_officer_review', 'act', 'Act on account_officer_review stage'),
  ('stage:account_supervisor_review:act', 'stage', 'account_supervisor_review', 'act', 'Act on account_supervisor_review stage'),
  ('stage:accounting_review:act',         'stage', 'accounting_review', 'act', 'Act on accounting_review stage'),
  ('stage:cfo_review:act',                'stage', 'cfo_review',        'act', 'Act on cfo_review stage'),
  ('stage:ceo_review:act',                'stage', 'ceo_review',        'act', 'CEO override / act on ceo_review stage'),
  ('stage:finance_review:act',            'stage', 'finance_review',    'act', 'Finance review / disburse at finance_review'),
  ('stage:po_pending:act',                'stage', 'po_pending',        'act', 'Act on PO pending'),
  ('stage:po_cfo:act',                    'stage', 'po_cfo',            'act', 'CFO sign-off on PO'),

-- tile · tab/dashboard access (aggregated by tile id)
  ('tile:pr:view',                'tile', 'pr',         'view', 'PR tab'),
  ('tile:ledger:view',            'tile', 'ledger',     'view', 'Ledger tab'),
  ('tile:cockpit:view',           'tile', 'cockpit',    'view', 'Cockpit tab (executive)'),
  ('tile:policy:view',            'tile', 'policy',     'view', 'Policy tab'),
  ('tile:settings:view',          'tile', 'settings',   'view', 'Settings tab'),
  ('tile:hr:view',                'tile', 'hr',         'view', 'HR tab'),
  ('tile:submit_expense:view',    'tile', 'submit_expense', 'view', 'Submit Expense tile'),
  ('tile:my_history:view',        'tile', 'my_history', 'view', 'My submissions tile'),
  ('tile:my_prs:view',            'tile', 'my_prs',     'view', 'My PRs tile'),
  ('tile:search_coa:view',        'tile', 'search_coa', 'view', 'Search COA tile'),
  ('tile:review_queue:view',      'tile', 'review_queue','view','Review queue tile'),
  ('tile:approve_expense:view',   'tile', 'approve_expense','view','Approve expense tile'),
  ('tile:all_approvals:view',     'tile', 'all_approvals','view', 'All approvals tile'),
  ('tile:directory:view',         'tile', 'directory',  'view', 'User directory tile'),
  ('tile:org_chart:view',         'tile', 'org_chart',  'view', 'Org chart tile'),
  ('tile:departments:view',       'tile', 'departments','view', 'Departments tile'),
  ('tile:access_requests:view',   'tile', 'access_requests','view','Access requests tile'),
  ('tile:dash_it:view',           'tile', 'dash_it',    'view', 'IT dashboard'),
  ('tile:dash_exec:view',         'tile', 'dash_exec',  'view', 'Executive dashboard'),
  ('tile:dash_manager:view',      'tile', 'dash_manager','view', 'Manager-of-dept dashboard'),
  ('tile:dash_am:view',           'tile', 'dash_am',    'view', 'Accounting manager dashboard'),
  ('tile:dash_reviewer:view',     'tile', 'dash_reviewer','view','Reviewer dashboard'),
  ('tile:dash_staff:view',        'tile', 'dash_staff', 'view', 'Staff dashboard'),
  ('tile:dash_hr:view',           'tile', 'dash_hr',    'view', 'HR dashboard'),
  ('tile:dash_finance:view',      'tile', 'dash_finance','view','Finance dashboard'),
  ('tile:hook_view:view',         'tile', 'hook_view',  'view', 'Hook events list'),

-- hook · webhook replay
  ('hook:event:view',             'hook',  'event', 'view',   'View hook events'),
  ('hook:event:replay',           'hook',  'event', 'replay', 'Replay hook event'),

-- ai · provider/model/staff/assignment/invocation management
  ('ai:provider:read',            'ai', 'provider',   'read',   'List AI providers'),
  ('ai:provider:create',          'ai', 'provider',   'create', 'Create AI provider'),
  ('ai:provider:update',          'ai', 'provider',   'update', 'Edit AI provider'),
  ('ai:provider:delete',          'ai', 'provider',   'delete', 'Delete AI provider'),
  ('ai:provider:test',            'ai', 'provider',   'test',   'Test an AI provider connection'),
  ('ai:model:read',               'ai', 'model',      'read',   'List AI models'),
  ('ai:model:create',             'ai', 'model',      'create', 'Create AI model'),
  ('ai:model:update',             'ai', 'model',      'update', 'Edit AI model'),
  ('ai:model:delete',             'ai', 'model',      'delete', 'Delete AI model'),
  ('ai:staff:read',               'ai', 'staff',      'read',   'List AI staff'),
  ('ai:staff:create',             'ai', 'staff',      'create', 'Create AI staff'),
  ('ai:staff:update',             'ai', 'staff',      'update', 'Edit AI staff'),
  ('ai:staff:delete',             'ai', 'staff',      'delete', 'Delete AI staff'),
  ('ai:staff:invoke',             'ai', 'staff',      'invoke', 'Invoke AI staff (test chat)'),
  ('ai:assignment:read',          'ai', 'assignment', 'read',   'List AI assignments'),
  ('ai:assignment:create',        'ai', 'assignment', 'create', 'Create AI assignment'),
  ('ai:assignment:delete',        'ai', 'assignment', 'delete', 'Delete AI assignment'),
  ('ai:invocation:view',          'ai', 'invocation', 'view',   'View AI invocation log'),
  ('ai:section_health:view',      'ai', 'section_health', 'view', 'View AI section health'),

-- policy · approval policy
  ('policy:approval:read',        'policy','approval','read',   'Read approval policies'),
  ('policy:approval:edit',        'policy','approval','edit',   'Create/edit approval policies'),
  ('policy:approval:delete',      'policy','approval','delete', 'Delete approval policies'),

-- access_request · request list/resolve
  ('access_request:request:list',     'access_request','request','list',    'List access requests'),
  ('access_request:request:resolve',  'access_request','request','resolve', 'Resolve access requests')
ON CONFLICT (id) DO NOTHING;

COMMIT;
