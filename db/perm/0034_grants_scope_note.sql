-- Folio — scope documentation for rbac.perm_grants.
--
-- This migration is documentation-only; it makes no schema change.
--
-- AS OF 0031_tile_access_gates.sql, the tile-view gate is purely:
--   user.staff_level <= tile.required_level
--   AND
--   user.dept_group_id = tile.required_dept_id
--
-- NULL on either axis = wildcard (no gate on that axis).
-- The runtime gate lives in app/src/components/tileAccess.server.ts:gateOk()
-- and does NOT consult:
--   - role permissions (perm.role_permissions)
--   - acting grants (rbac.perm_grants)
--   - admin:system:bypass:all
--
-- As a result, no per-user dynamic assignment can unlock a tile.
-- To grant access to a tile, HR must adjust the tile's required_level
-- or required_dept_id via the /tiles admin page (or SQL UPDATE on perm.tiles).
--
-- rbac.perm_grants is still maintained for:
--   - Head-of-department: dept:X:head:N:all perms (assigned via /api/departments PATCH)
--   - Acting-as bundles: role perm grants for a time window
--     (lib/perm/grants.ts:grantActingBundle)
--   - User deactivation cascade: /api/users/[id] DELETE revokes all grants
--
-- These grants affect mutation perms (e.g. who can approve at which stage).
-- They do not affect tile visibility.

SELECT
  'rbac.perm_grants does not influence tile view' AS invariant,
  (SELECT count(*) FROM rbac.perm_grants WHERE revoked_at IS NULL) AS active_grants,
  (SELECT count(*) FROM perm.tiles) AS tile_count;
