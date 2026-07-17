-- folio/db/migrations/2026-07-22-B-cashflow-and-sql-perms.sql
--
-- New permissions:
--   finance:cashflow:read::allow   view a posted cash-flow statement
--   ai:sql:ask::allow              ask the chat-to-SQL helper
-- Granted to CFO + finance lead + admin via the curated baseline bundles.

BEGIN;

INSERT INTO perm.permissions (id, description) VALUES
  ('finance:cashflow:read::allow', 'View a posted cash-flow statement'),
  ('ai:sql:ask::allow',            'Ask the chat-to-SQL helper')
ON CONFLICT (id) DO NOTHING;

INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT r.id, p.id, 'cashflow-and-sql-baseline'
  FROM perm.roles r
 CROSS JOIN perm.permissions p
 WHERE p.id IN ('finance:cashflow:read::allow', 'ai:sql:ask::allow')
   AND r.id IN ('cfo::2','finance::2','accounting_manager::3','account_supervisor::4','account_officer::5','admin::2','ceo::1')
ON CONFLICT DO NOTHING;

INSERT INTO perm.audit (kind, target) VALUES (
  'schema.migration',
  jsonb_build_object(
    'migration', '2026-07-22-B-cashflow-and-sql-perms',
    'changes', jsonb_build_array(
      'finance:cashflow:read::allow',
      'ai:sql:ask::allow'
    )
  )
);

COMMIT;