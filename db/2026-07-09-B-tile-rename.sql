-- db/2026-07-09-B-tile-rename.sql
--
-- L-3: rename legacy tile IDs to canonical.
--   expense-claim        → expense        (and href, request_target)
--   approve-expense      → DELETE         (merged into expense tab=approve)
--   my-prs               → pr             (and href, request_target)
--   all-approvals        → my-waybills    (and href, request_target)
--   po                   → po             (kept — already canonical)
--
-- perm.roles (group-name spaces) and perm.role_permissions must follow.
-- The /po tile stays; we only adjust its href if it points somewhere stale.

BEGIN;

UPDATE perm.tiles
   SET id              = 'expense',
       href            = '/expense',
       request_target  = 'tile:expense:view',
       subtitle        = 'Waybills · submit, track, settle',
       display_name    = 'Expense'
 WHERE id = 'expense-claim';

UPDATE perm.tiles
   SET id              = 'pr',
       href            = '/pr',
       request_target  = 'tile:pr:view',
       subtitle        = 'Purchase requisitions · Waybill',
       display_name    = 'PR'
 WHERE id = 'my-prs';

UPDATE perm.tiles
   SET id              = 'my-waybills',
       href            = '/my-waybills',
       request_target  = 'tile:my-waybills:view',
       subtitle        = 'All open Waybills · filter by scope',
       display_name    = 'My Waybills'
 WHERE id = 'all-approvals';

DELETE FROM perm.tiles WHERE id = 'approve-expense';

UPDATE perm.roles
   SET id = REPLACE(
          REPLACE(
            REPLACE(
              REPLACE(id, 'tile-submit-expense', 'tile-expense'),
              'tile-approve-expense','tile-expense'),
            'tile-my-prs', 'tile-pr'),
          'tile-all-approvals', 'tile-my-waybills')
 WHERE id LIKE 'tile-%';

UPDATE perm.role_permissions
   SET role_id = REPLACE(
          REPLACE(
            REPLACE(
              REPLACE(role_id, 'tile-submit-expense', 'tile-expense'),
              'tile-approve-expense','tile-expense'),
            'tile-my-prs', 'tile-pr'),
          'tile-all-approvals', 'tile-my-waybills')
 WHERE role_id LIKE 'tile-%';

INSERT INTO perm.audit (kind, actor, target)
VALUES (
  'schema.migration',
  'system',
  jsonb_build_object(
    'migration', '2026-07-09-B-tile-rename',
    'renames', jsonb_build_object(
      'expense-claim',  'expense',
      'my-prs',         'pr',
      'all-approvals',  'my-waybills',
      'approve-expense','deleted'
    )
  )
);

COMMIT;