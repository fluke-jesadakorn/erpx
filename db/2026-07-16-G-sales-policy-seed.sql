-- db/2026-07-16-G-sales-policy-seed.sql
--
-- Seed new POL.* ids in perm.policies so the registry ref() lookups resolve
-- for sales-side stage transitions.
--
-- The actual AST evaluation lives in lib/policy/registry.ts (salesStagePolicy()).
-- This migration only registers the canonical ids so policy_decisions audit
-- rows can record them.
--
-- Run with:
--   PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db \
--     -v ON_ERROR_STOP=1 -f folio/db/2026-07-16-G-sales-policy-seed.sql

BEGIN;

INSERT INTO perm.policies (id, name, ast, description) VALUES
  ('viewSalesOrder',        'View sales order',     '{"kind":"ref","id":"viewSalesOrder"}'::jsonb,  'Allow viewing sales orders (sales + finance)'),
  ('canSettleSales',        'Settle sales order',   '{"kind":"ref","id":"canSettleSales"}'::jsonb,  'Attach AR receipt at so_paid'),
  ('canPostSalesGlVat',     'Post sales GL VAT',    '{"kind":"ref","id":"canPostSalesGlVat"}'::jsonb, 'Post VAT line at so_invoiced'),
  ('canPostSalesGlAccrual', 'Post sales GL accrual','{"kind":"ref","id":"canPostSalesGlAccrual"}'::jsonb, 'Post revenue+AR at so_invoiced'),
  ('canPostSalesGlSettlement', 'Post sales GL settle', '{"kind":"ref","id":"canPostSalesGlSettlement"}'::jsonb, 'Post cash+AR clear at so_paid'),
  ('canConfirmSalesGl',     'Confirm sales GL',     '{"kind":"ref","id":"canConfirmSalesGl"}'::jsonb,'Confirm GL after so_paid'),
  ('canManageCustomer',     'Manage customer',      '{"kind":"ref","id":"canManageCustomer"}'::jsonb,'Create/update customer master'),
  ('viewCustomer',          'View customer',        '{"kind":"ref","id":"viewCustomer"}'::jsonb,     'View customer master + AR history'),
  ('canActOnSalesOrder',    'Act on sales order',   '{"kind":"ref","id":"canActOnSalesOrder"}'::jsonb,'Generic sales stage action gate')
ON CONFLICT (id) DO NOTHING;

INSERT INTO perm.audit (kind, actor, target)
VALUES (
  'schema.migration',
  'system',
  jsonb_build_object(
    'migration', '2026-07-16-G-sales-policy-seed',
    'description', '9 sales-policy ids registered in perm.policies'
  )
);

COMMIT;
