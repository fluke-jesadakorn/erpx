-- Folio — seed perm.role_permissions
-- Curated per-role grant set aligned with the current runtime matrix.
-- Granular tuning can be done via /perm/matrix after migration.

BEGIN;

-- admin: every permission (full effect allow)
INSERT INTO perm.role_permissions (role_id, permission_id, effect, granted_by)
SELECT 'admin', p.id, 'allow', 'seed-0004'
FROM perm.permissions p
ON CONFLICT DO NOTHING;

-- cfo, ceo, it: get all finance:r/w + tile:* + rbac:matrix:view + hook:*
INSERT INTO perm.role_permissions (role_id, permission_id, effect, granted_by)
SELECT r.id, p.id, 'allow', 'seed-0004'
FROM (VALUES ('cfo'), ('ceo'), ('it')) AS r(id)
CROSS JOIN LATERAL (
  SELECT id FROM perm.permissions
  WHERE domain IN ('finance','tile','stage','hook','rbac','org','ai','policy','user')
) p
ON CONFLICT DO NOTHING;

-- finance: finance + tile-ledger + finance:report:executive + stage:finance_review:act
INSERT INTO perm.role_permissions (role_id, permission_id, effect, granted_by)
SELECT 'finance', p.id, 'allow', 'seed-0004'
FROM perm.permissions p
WHERE p.id IN (
  'finance:expense:view_all:all','finance:expense:settle:all','finance:expense:disburse:all',
  'finance:expense:reject:all','finance:po:approve:all','finance:po:settle:all',
  'finance:po:reject:all','finance:ledger:view:all','finance:reconciliation:view:all',
  'finance:report:executive:all','tile:ledger:view:all',
  'tile:cockpit:view:all','tile:dash_exec:view:all','tile:dash_finance:view:all',
  'tile:review_queue:view:all','tile:approve_expense:view:all','tile:all_approvals:view:all',
  'tile:hook_view:view:all',
  'stage:finance_review:act:all','stage:po_cfo:act:all'
)
ON CONFLICT DO NOTHING;

-- accounting_manager (L3): review/approve at accounting_review + general expense view
INSERT INTO perm.role_permissions (role_id, permission_id, effect, granted_by)
SELECT 'accounting_manager', p.id, 'allow', 'seed-0004'
FROM perm.permissions p
WHERE p.id IN (
  'finance:expense:view_all:all','finance:expense:review:all','finance:expense:approve:all',
  'finance:expense:reject:all',
  'tile:pr:view:all','tile:ledger:view:all','tile:cockpit:view:all',
  'tile:dash_am:view:all','tile:dash_manager:view:all','tile:review_queue:view:all',
  'tile:approve_expense:view:all','tile:all_approvals:view:all',
  'stage:accounting_review:act:all','stage:manager_review:act:all','stage:po_pending:act:all',
  'rbac:matrix:view:all','user:subtree:edit:all'
)
ON CONFLICT DO NOTHING;

-- manager (L3): approve manager_review + view subtree
INSERT INTO perm.role_permissions (role_id, permission_id, effect, granted_by)
SELECT 'manager', p.id, 'allow', 'seed-0004'
FROM perm.permissions p
WHERE p.id IN (
  'finance:expense:view_all:all','finance:expense:approve:all',  'finance:expense:reject:all',
  'tile:pr:view:all','tile:cockpit:view:all',
  'tile:dash_manager:view:all','tile:review_queue:view:all',
  'stage:manager_review:act:all','stage:po_pending:act:all',
  'user:subtree:edit:all','rbac:matrix:view:all'
)
ON CONFLICT DO NOTHING;

-- supervisor (L2B): supervisor_review stage + view own dept
INSERT INTO perm.role_permissions (role_id, permission_id, effect, granted_by)
SELECT 'supervisor', p.id, 'allow', 'seed-0004'
FROM perm.permissions p
WHERE p.id IN (
  'finance:expense:view_all:all','finance:expense:approve:all','finance:expense:reject:all',
  'tile:pr:view:all','tile:dash_reviewer:view:all',
  'tile:review_queue:view:all',
  'stage:supervisor_review:act:all'
)
ON CONFLICT DO NOTHING;

-- account_officer (L2B): account_officer_review stage
INSERT INTO perm.role_permissions (role_id, permission_id, effect, granted_by)
SELECT 'account_officer', p.id, 'allow', 'seed-0004'
FROM perm.permissions p
WHERE p.id IN (
  'finance:expense:view_all:all','finance:expense:approve:all',  'finance:expense:reject:all',
  'tile:pr:view:all','tile:dash_reviewer:view:all',
  'tile:review_queue:view:all',
  'stage:account_officer_review:act:all'
)
ON CONFLICT DO NOTHING;

-- account_supervisor (L2A): account_supervisor_review stage
INSERT INTO perm.role_permissions (role_id, permission_id, effect, granted_by)
SELECT 'account_supervisor', p.id, 'allow', 'seed-0004'
FROM perm.permissions p
WHERE p.id IN (
  'finance:expense:view_all:all','finance:expense:approve:all',  'finance:expense:reject:all',
  'tile:pr:view:all','tile:dash_reviewer:view:all',
  'tile:review_queue:view:all',
  'stage:account_supervisor_review:act:all'
)
ON CONFLICT DO NOTHING;

-- accountant (L2A): review expense (OCR → accountant_reviewed)
INSERT INTO perm.role_permissions (role_id, permission_id, effect, granted_by)
SELECT 'accountant', p.id, 'allow', 'seed-0004'
FROM perm.permissions p
WHERE p.id IN (
  'finance:expense:view_all:all',  'finance:expense:review:all',
  'tile:ledger:view:all','tile:dash_reviewer:view:all',
  'tile:review_queue:view:all'
)
ON CONFLICT DO NOTHING;

-- staff (L2A): submit expenses, view own
INSERT INTO perm.role_permissions (role_id, permission_id, effect, granted_by)
SELECT 'staff', p.id, 'allow', 'seed-0004'
FROM perm.permissions p
WHERE p.id IN (
  'finance:expense:view_own:all','finance:expense:create:all',
  'finance:pr:create:all',
  'tile:submit_expense:view:all','tile:my_history:view:all',
  'tile:my_prs:view:all','tile:dash_staff:view:all','tile:search_coa:view:all'
)
ON CONFLICT DO NOTHING;

-- hr (L2A): user read, dept read
INSERT INTO perm.role_permissions (role_id, permission_id, effect, granted_by)
SELECT 'hr', p.id, 'allow', 'seed-0004'
FROM perm.permissions p
WHERE p.id IN (
  'user:directory:read:all','user:role:assign:all','user:dept:edit:all',
  'org:tree:view:all','org:dept:read:all',
  'tile:hr:view:all','tile:directory:view:all',
  'tile:org_chart:view:all','tile:departments:view:all','tile:dash_hr:view:all',
  'access_request:request:list:all','access_request:request:resolve:all',
  'rbac:matrix:view:all'
)
ON CONFLICT DO NOTHING;

-- hr_manager (L2A): everything hr does + edit users + subtree + hook
INSERT INTO perm.role_permissions (role_id, permission_id, effect, granted_by)
SELECT 'hr_manager', p.id, 'allow', 'seed-0004'
FROM perm.permissions p
WHERE p.id IN (
  'user:directory:read:all','user:profile:create:all','user:profile:update:all',
  'user:profile:delete:all','user:profile:deactivate:all','user:role:assign:all',
  'user:manager:set:all','user:dept:edit:all','user:subtree:edit:all',
  'org:tree:view:all','org:dept:read:all','org:dept:assign_head:all',
  'org:auto_wire:propose:all','org:auto_wire:apply:all',
  'tile:hr:view:all','tile:directory:view:all',
  'tile:org_chart:view:all','tile:departments:view:all','tile:dash_hr:view:all',
  'access_request:request:list:all','access_request:request:resolve:all',
  'rbac:matrix:view:all','rbac:matrix:edit:all'
)
ON CONFLICT DO NOTHING;

-- L4 (legacy root level): full system
INSERT INTO perm.role_permissions (role_id, permission_id, effect, granted_by)
SELECT 'L4', p.id, 'allow', 'seed-0004'
FROM perm.permissions p
ON CONFLICT DO NOTHING;

-- L3 (legacy dept-manager level): matches accounting_manager grants
INSERT INTO perm.role_permissions (role_id, permission_id, effect, granted_by)
SELECT 'L3', p.id, 'allow', 'seed-0004'
FROM perm.permissions p
WHERE p.id IN (
  'finance:expense:view_all:all','finance:expense:review:all','finance:expense:approve:all',
  'finance:expense:reject:all',
  'tile:pr:view:all','tile:ledger:view:all','tile:cockpit:view:all',
  'tile:dash_am:view:all','tile:dash_manager:view:all',
  'tile:review_queue:view:all','tile:approve_expense:view:all','tile:all_approvals:view:all',
  'stage:accounting_review:act:all','stage:manager_review:act:all','stage:po_pending:act:all',
  'rbac:matrix:view:all','user:subtree:edit:all'
)
ON CONFLICT DO NOTHING;

-- L2B (legacy supervisor level): combined supervisor + account_officer grants
INSERT INTO perm.role_permissions (role_id, permission_id, effect, granted_by)
SELECT 'L2B', p.id, 'allow', 'seed-0004'
FROM perm.permissions p
WHERE p.id IN (
  'finance:expense:view_all:all','finance:expense:approve:all',  'finance:expense:reject:all',
  'tile:pr:view:all','tile:dash_reviewer:view:all',
  'tile:review_queue:view:all',
  'stage:supervisor_review:act:all','stage:account_officer_review:act:all'
)
ON CONFLICT DO NOTHING;

-- L2A (legacy operational level): combined staff + accountant grants
INSERT INTO perm.role_permissions (role_id, permission_id, effect, granted_by)
SELECT 'L2A', p.id, 'allow', 'seed-0004'
FROM perm.permissions p
WHERE p.id IN (
  'finance:expense:view_own:all','finance:expense:create:all','finance:expense:view_all:all',
  'finance:expense:review:all',  'finance:pr:create:all',
  'tile:pr:view:all','tile:hr:view:all','tile:ledger:view:all',
  'tile:submit_expense:view:all','tile:my_history:view:all','tile:my_prs:view:all',
  'tile:dash_staff:view:all','tile:dash_hr:view:all','tile:dash_reviewer:view:all',
  'tile:search_coa:view:all',
  'user:directory:read:all','user:role:assign:all','user:dept:edit:all',
  'org:tree:view:all','org:dept:read:all',
  'access_request:request:list:all','access_request:request:resolve:all',
  'rbac:matrix:view:all','rbac:matrix:edit:all',
  'stage:account_supervisor_review:act:all'
)
ON CONFLICT DO NOTHING;

-- L1 (legacy intern/guest): read-only
INSERT INTO perm.role_permissions (role_id, permission_id, effect, granted_by)
SELECT 'L1', p.id, 'allow', 'seed-0004'
FROM perm.permissions p
WHERE false
ON CONFLICT DO NOTHING;

COMMIT;
