-- db/2026-07-16-B-sales-roles.sql
--
-- Add sales_rep + sales_supervisor personas to perm.roles.
-- These are added alongside existing 14 personas — no existing role is changed.
--
-- Run with:
--   PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db \
--     -v ON_ERROR_STOP=1 -f folio/db/2026-07-16-B-sales-roles.sql

BEGIN;

INSERT INTO perm.roles (id, kind, display_name, display_name_de, display_name_th, description, is_system, sort_order)
VALUES
  ('sales_rep',        'persona', 'Sales Rep',        'Verkaufsmitarbeiter',  'เซลล์',           'Submits and drafts sales orders',         false, 316),
  ('sales_supervisor', 'persona', 'Sales Supervisor', 'Verkaufsleiter',      'หัวหน้าทีมขาย',   'Reviews pricing + issues Tax Invoice',     false, 317)
ON CONFLICT (id) DO UPDATE SET
  display_name    = EXCLUDED.display_name,
  display_name_de = EXCLUDED.display_name_de,
  display_name_th = EXCLUDED.display_name_th,
  description     = EXCLUDED.description,
  sort_order      = EXCLUDED.sort_order;

INSERT INTO perm.audit (kind, actor, target)
VALUES (
  'schema.migration',
  'system',
  jsonb_build_object(
    'migration', '2026-07-16-B-sales-roles',
    'description', 'sales_rep + sales_supervisor personas added to perm.roles'
  )
);

COMMIT;
