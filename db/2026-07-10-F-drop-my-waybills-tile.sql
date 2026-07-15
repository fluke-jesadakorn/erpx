-- folio/db/2026-07-10-F-drop-my-waybills-tile.sql
--
-- The /expense tile now hosts the open-Waybills inbox (moved from
-- /my-waybills). Drop the redundant my-waybills tile so only one entry
-- shows on the Hub. The /my-waybills route stays as a thin redirect to
-- /expense?scope=… for any leftover links/email.

BEGIN;

DELETE FROM perm.tiles WHERE id = 'my-waybills';

INSERT INTO perm.audit (kind, target)
VALUES (
  'schema.migration',
  jsonb_build_object(
    'migration', '2026-07-10-F-drop-my-waybills-tile',
    'tiles_removed', jsonb_build_array('my-waybills'),
    'reason', 'Expense tile now hosts the open-Waybills inbox'
  )
);

COMMIT;
