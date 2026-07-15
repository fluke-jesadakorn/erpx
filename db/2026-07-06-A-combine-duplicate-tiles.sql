-- 2026-07-06-A: combine duplicate tiles into single pages, add History scope to canonicals.
--
-- 5 tiles are redundant with their canonical pair; fold them in and let the
-- FeatureDispatch tabs switch on tile.sub_view + local state instead of slug.
-- Old slugs return 404 — no redirects.
--
-- Order matters: tiles must be deleted before their permissions (FK constraint
-- on tiles.required_permission → permissions.id is ON DELETE RESTRICT).
-- role_permissions rows for the duplicate permissions are safe to drop — all 5
-- permission rows had the same 27 role grants as their canonical pair, so the
-- canonical pair still grants the same roles access to the merged feature.
--
--  ┌────────────────────┐   absorbs   ┌────────────────────┐
--  │ submit-expense     │◀───────────│ my-history          │
--  │ search-coa         │◀───────────│ search-slips        │
--  │ approve-expense    │◀───────────│ review-queue        │
--  │ my-prs             │◀───────────│ subordinate-prs,    │
--  │                    │            │ all-prs             │
--  └────────────────────┘             └─────────────────────┘

BEGIN;

-- 1) Rename canonical tiles to reflect the merged scope.
UPDATE perm.tiles SET display_name = 'Submit Slip',
                       subtitle     = 'Submit & track reimbursements'
 WHERE id = 'submit-expense';

UPDATE perm.tiles SET display_name = 'Search',
                       subtitle     = 'COA & receipts'
 WHERE id = 'search-coa';

UPDATE perm.tiles SET display_name = 'Expense Approval',
                       subtitle     = 'Queue · Your stage · History'
 WHERE id = 'approve-expense';

UPDATE perm.tiles SET display_name = 'Purchase Requisitions',
                       subtitle     = 'Submit · Approve · Track'
 WHERE id = 'my-prs';

-- 2) Sort-order tweaks: nudge search-coa up so the merge reads naturally.
UPDATE perm.tiles SET sort_order = 180 WHERE id = 'search-coa';

-- 3) Drop the duplicate tile rows first (unlinks the FK before we drop
--    the permission rows).
DELETE FROM perm.tiles
 WHERE id IN (
   'my-history',
   'search-slips',
   'review-queue',
   'subordinate-prs',
   'all-prs'
 );

-- 4) Drop the duplicate role_permission grants. They are identical to the
--    canonical grants (verified — same 27 roles per perm), so this is a
--    no-op for access decisions.
DELETE FROM perm.role_permissions
 WHERE permission_id IN (
   'tile:my_history:view:all',
   'tile:search_slips:view:all',
   'tile:review_queue:view:all',
   'tile:subordinate_prs:view:all',
   'tile:all_prs:view:all'
 );

-- 5) Drop the duplicate permission rows.
DELETE FROM perm.permissions
 WHERE id IN (
   'tile:my_history:view:all',
   'tile:search_slips:view:all',
   'tile:review_queue:view:all',
   'tile:subordinate_prs:view:all',
   'tile:all_prs:view:all'
 );

-- 6) Sanity check: row counts.
--    (Expected: 29 → 24 tiles, 5 fewer permissions.)
--    Run separately:
--      SELECT COUNT(*) FROM perm.tiles;
--      SELECT COUNT(*) FROM perm.permissions WHERE id LIKE 'tile:%';

COMMIT;
