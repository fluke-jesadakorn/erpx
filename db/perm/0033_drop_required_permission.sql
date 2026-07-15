-- Folio — drop perm.tiles.required_permission.
--
-- The column was used by the old perm-grant gate
-- (`hasPermission(session, tile.required_permission)`) which has been replaced
-- by the static (required_level, required_dept_id) gate in
-- 0031_tile_access_gates.sql. The column is now purely advisory and no
-- production code reads it.
--
-- Reversible: re-add the column from 0018_tiles_table.sql's original
-- definition. Tile data is unaffected.

BEGIN;

-- Drop FK to perm.permissions first (the column's only constraint)
ALTER TABLE perm.tiles
  DROP CONSTRAINT IF EXISTS tiles_required_permission_fkey;

-- Drop the index that referenced this column
DROP INDEX IF EXISTS perm_tiles_required_perm_idx;

-- Drop the column itself
ALTER TABLE perm.tiles
  DROP COLUMN IF EXISTS required_permission;

COMMIT;
