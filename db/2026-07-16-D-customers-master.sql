-- db/2026-07-16-D-customers-master.sql
--
-- Minimal fallback for legacy installs: customers, customer_contacts tables +
-- customer_ar_history view are created here (already in 2026-07-16-A but split
-- migration kept for parity). This migration is idempotent.
--
-- Run with:
--   PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db \
--     -v ON_ERROR_STOP=1 -f folio/db/2026-07-16-D-customers-master.sql
--
-- All tables/views in this file are already created by 2026-07-16-A. This file
-- is reserved for future customer extensions (e.g., tax_id_blacklist view, AR
-- statement template). For now: just an audit row.

BEGIN;

INSERT INTO perm.audit (kind, actor, target)
VALUES (
  'schema.migration',
  'system',
  jsonb_build_object(
    'migration', '2026-07-16-D-customers-master',
    'description', 'No-op (customers tables created in 2026-07-16-A); reserved for future customer extensions'
  )
);

COMMIT;
