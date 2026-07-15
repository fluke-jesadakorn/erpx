-- db/2026-07-16-E-sales-tile-and-perms.sql
--
-- Add 'sales' + 'customers' tiles to perm.tiles
-- Add new perm strings: stage:so_*:act:all, finance:sales:*, tile:sales:view, tile:customers:view,
--                       rbac:level:grant:min:2:all, rbac:level:grant:min:3:all
-- Grant perms to sales_rep, sales_supervisor, account_officer, accounting_manager, finance, cfo
--
-- Run with:
--   PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db \
--     -v ON_ERROR_STOP=1 -f folio/db/2026-07-16-E-sales-tile-and-perms.sql

BEGIN;

-- ============================================================================
-- 1. New perm strings
-- ============================================================================

INSERT INTO perm.permissions (domain, subject, verb, scope, description) VALUES
  ('stage',  'so_draft',         'act', 'all', 'Sales draft stage — sales_rep composes'),
  ('stage',  'so_sales_review',  'act', 'all', 'Sales supervisor reviews pricing + customer + qty'),
  ('stage',  'so_credit_check',  'act', 'all', 'Account + sales supervisor verify AR + credit limit'),
  ('stage',  'so_invoiced',      'act', 'all', 'Accounting manager issues Tax Invoice (GL VAT+Accrual)'),
  ('stage',  'so_paid',          'act', 'all', 'Finance attaches AR receipt (GL Settlement)'),
  ('finance','sales',            'create',  'self', 'Sales rep can create + edit own SO drafts'),
  ('finance','sales',            'approve', 'dept', 'Sales supervisor approves at sales_review + credit_check'),
  ('finance','sales',            'approve', 'all',  'CFO/finance override for cross-dept SOs'),
  ('finance','sales',            'settle',  'all',  'Finance records AR receipt at so_paid'),
  ('finance','sales',            'settle',  'self', 'Sales rep can record own settlement on small SOs'),
  ('finance','sales',            'gl_confirm', 'all', 'Post + confirm sales GL journals (VAT, accrual, settlement)'),
  ('finance','customer',         'view',    'self', 'Sales rep can view own customers'),
  ('finance','customer',         'edit',    'self', 'Sales rep can edit own customers'),
  ('finance','customer',         'edit',    'all',  'Cross-dept customer CRUD'),
  ('finance','customer',         'view',    'all',  'View customer master + AR history'),
  ('tile',   'sales',            'view',    'all',  'Sales tile visibility'),
  ('tile',   'customers',        'view',    'all',  'Customers tile visibility'),
  ('rbac',   'level',            'grant:min:3', 'all', 'sales_rep can grant level >= 3'),
  ('rbac',   'level',            'grant:min:2', 'all', 'sales_supervisor can grant level >= 2')
ON CONFLICT (domain, subject, verb, scope) DO NOTHING;

-- ============================================================================
-- 2. Sales + Customers tiles
-- ============================================================================

INSERT INTO perm.tiles (id, display_name, subtitle, icon, accent, group_name, sub_view, href, sort_order, required_level, is_system)
VALUES
  ('sales',     'Sales Orders',   'Customer SOs · AR pipeline', '🛒', 'emerald', 'workflow', NULL,       '/sales',     125, 3, true),
  ('customers', 'Customers',      'Customer master · AR history','🏢','cyan',    'workflow', 'directory','/customers', 130, 2, true)
ON CONFLICT (id) DO UPDATE SET
  display_name   = EXCLUDED.display_name,
  subtitle       = EXCLUDED.subtitle,
  icon           = EXCLUDED.icon,
  accent         = EXCLUDED.accent,
  group_name     = EXCLUDED.group_name,
  sub_view       = EXCLUDED.sub_view,
  href           = EXCLUDED.href,
  sort_order     = EXCLUDED.sort_order,
  required_level = EXCLUDED.required_level,
  is_system      = EXCLUDED.is_system;

-- ============================================================================
-- 3. Grants for sales_rep
-- ============================================================================

INSERT INTO perm.role_permissions (role_id, permission_id, effect)
SELECT 'sales_rep', p.id, 'allow'
  FROM perm.permissions p
 WHERE p.id IN (
   'tile:sales:view:all',
   'tile:customers:view:all',
   'tile:submit_expense:view:all',
   'tile:my_history:view:all',
   'tile:my_prs:view:all',
   'tile:search_coa:view:all',
   'tile:dash_staff:view:all',
   'finance:sales:create:self',
   'finance:sales:settle:self',
   'finance:customer:view:self',
   'finance:customer:edit:self',
   'finance:customer:view:all',
   'stage:so_draft:act:all',
   'rbac:level:grant:min:3:all'
 )
ON CONFLICT DO NOTHING;

-- ============================================================================
-- 4. Grants for sales_supervisor
-- ============================================================================

INSERT INTO perm.role_permissions (role_id, permission_id, effect)
SELECT 'sales_supervisor', p.id, 'allow'
  FROM perm.permissions p
 WHERE p.id IN (
   'tile:sales:view:all',
   'tile:customers:view:all',
   'tile:submit_expense:view:all',
   'tile:my_history:view:all',
   'tile:my_prs:view:all',
   'tile:search_coa:view:all',
   'tile:dash_reviewer:view:all',
   'tile:dash_manager:view:all',
   'tile:cockpit:view:all',
   'finance:sales:create:self',
   'finance:sales:approve:dept',
   'finance:sales:approve:all',
   'finance:sales:settle:all',
   'finance:sales:gl_confirm:all',
   'finance:customer:view:self',
   'finance:customer:edit:self',
   'finance:customer:edit:all',
   'finance:customer:view:all',
   'stage:so_draft:act:all',
   'stage:so_sales_review:act:all',
   'stage:so_credit_check:act:all',
   'stage:so_invoiced:act:all',
   'rbac:level:grant:min:2:all'
 )
ON CONFLICT DO NOTHING;

-- ============================================================================
-- 5. Grants for account_officer + accounting_manager (credit + invoice)
-- ============================================================================

INSERT INTO perm.role_permissions (role_id, permission_id, effect)
SELECT r.id, p.id, 'allow'
  FROM perm.roles r
  JOIN perm.permissions p
    ON p.id IN (
      'tile:sales:view:all',
      'tile:customers:view:all',
      'finance:customer:view:all',
      'stage:so_credit_check:act:all'
    )
 WHERE r.id = 'account_officer'
ON CONFLICT DO NOTHING;

INSERT INTO perm.role_permissions (role_id, permission_id, effect)
SELECT r.id, p.id, 'allow'
  FROM perm.roles r
  JOIN perm.permissions p
    ON p.id IN (
      'tile:sales:view:all',
      'tile:customers:view:all',
      'finance:customer:edit:all',
      'finance:customer:view:all',
      'finance:sales:gl_confirm:all',
      'stage:so_invoiced:act:all',
      'stage:so_credit_check:act:all'
    )
 WHERE r.id = 'accounting_manager'
ON CONFLICT DO NOTHING;

-- ============================================================================
-- 6. Grants for finance (settle)
-- ============================================================================

INSERT INTO perm.role_permissions (role_id, permission_id, effect)
SELECT r.id, p.id, 'allow'
  FROM perm.roles r
  JOIN perm.permissions p
    ON p.id IN (
      'tile:sales:view:all',
      'tile:customers:view:all',
      'finance:customer:view:all',
      'finance:sales:settle:all',
      'finance:sales:gl_confirm:all',
      'stage:so_paid:act:all'
    )
 WHERE r.id = 'finance'
ON CONFLICT DO NOTHING;

-- ============================================================================
-- 7. Grants for cfo + ceo (cockpit view)
-- ============================================================================

INSERT INTO perm.role_permissions (role_id, permission_id, effect)
SELECT r.id, p.id, 'allow'
  FROM perm.roles r
  JOIN perm.permissions p
    ON p.id IN (
      'tile:sales:view:all',
      'tile:customers:view:all',
      'finance:customer:view:all',
      'finance:sales:gl_confirm:all',
      'finance:sales:approve:all'
    )
 WHERE r.id IN ('cfo', 'ceo')
ON CONFLICT DO NOTHING;

INSERT INTO perm.audit (kind, actor, target)
VALUES (
  'schema.migration',
  'system',
  jsonb_build_object(
    'migration', '2026-07-16-E-sales-tile-and-perms',
    'description', 'sales + customers tiles; stage:so_*:act + finance:sales:* perms + role grants'
  )
);

COMMIT;
