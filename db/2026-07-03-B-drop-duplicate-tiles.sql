-- 2026-07-03-B: drop redundant perm.tiles rows + fix sort_order collisions.
--
-- Two tiles serve the same user objective as an existing one; remove them:
--   * hook-inbox: same perm (tile:hook_view:view:all), same sort_order 340,
--     same group 'it' as 'hook'. Keep 'hook' (better subtitle).
--   * ops-overview: same user objective as 'workbench' (admin read-only overview
--     of all submissions). Keep 'workbench' (broader scope, clearer name).
--
-- Two tiles collide on sort_order with neighbours in the same group; bump:
--   * visibility 245 → 246 (after summary, before ledger)
--   * roles     290 → 291 (after org-chart, before directory)
--
-- ops-overview's perm (tile:ops_overview:view:all) has 0 role grants (verified
-- before delete), so no role loses access.

BEGIN;

DELETE FROM perm.tiles WHERE id = 'hook-inbox';
DELETE FROM perm.tiles WHERE id = 'ops-overview';

UPDATE perm.tiles SET sort_order = 246 WHERE id = 'visibility';
UPDATE perm.tiles SET sort_order = 291 WHERE id = 'roles';

COMMIT;