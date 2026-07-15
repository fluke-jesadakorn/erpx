-- db/2026-07-16-C-sales-department.sql
--
-- Create dept-sales department under dept-executive in the org tree.
-- Empty department (no users migrated). Marketing dept + 4 users are untouched.
--
-- Run with:
--   PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db \
--     -v ON_ERROR_STOP=1 -f folio/db/2026-07-16-C-sales-department.sql

BEGIN;

INSERT INTO perm.roles (id, kind, display_name, display_name_de, display_name_th, parent_role_id, description, is_system, sort_order, monthly_budget)
VALUES
  ('dept-sales', 'department', 'Sales', 'Vertrieb', 'ฝ่ายขาย', 'dept-executive',
   'Sales / Income department — handles customer SOs, AR aging, GL posting of revenue',
   false, 305, 0)
ON CONFLICT (id) DO UPDATE SET
  display_name    = EXCLUDED.display_name,
  display_name_de = EXCLUDED.display_name_de,
  display_name_th = EXCLUDED.display_name_th,
  parent_role_id  = EXCLUDED.parent_role_id,
  description     = EXCLUDED.description,
  sort_order      = EXCLUDED.sort_order;

INSERT INTO perm.audit (kind, actor, target)
VALUES (
  'schema.migration',
  'system',
  jsonb_build_object(
    'migration', '2026-07-16-C-sales-department',
    'description', 'dept-sales department created under dept-executive (empty — no user migration)'
  )
);

COMMIT;
