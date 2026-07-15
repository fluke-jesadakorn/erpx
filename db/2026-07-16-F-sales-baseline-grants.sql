-- db/2026-07-16-F-sales-baseline-grants.sql
--
-- Baseline grant extension for sales_rep + sales_supervisor following the
-- pattern of 0028_baseline_perms_for_all.sql + 0029_baseline_view_tiles_for_all.sql.
--
-- Run with:
--   PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db \
--     -v ON_ERROR_STOP=1 -f folio/db/2026-07-16-F-sales-baseline-grants.sql

BEGIN;

-- Empty department baseline — no auto-grants yet (kept for future ad-hoc
-- user-role assignments via UI). Sales persons receive their perms via role
-- (sales_rep / sales_supervisor), not via the dept-sales empty grant block.

INSERT INTO perm.audit (kind, actor, target)
VALUES (
  'schema.migration',
  'system',
  jsonb_build_object(
    'migration', '2026-07-16-F-sales-baseline-grants',
    'description', 'sales_rep + sales_supervisor baseline grants applied via E migration; this file reserves the dept-sales baseline block'
  )
);

COMMIT;
