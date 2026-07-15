-- Folio — tile access moves from perm-grant driven to static (level + dept) gates.
--
-- Each tile now declares its own `required_level` (1..5) and `required_dept_id` (FK to a
-- department-role). Access = user.staff_level <= required_level AND user.dept_group_id =
-- required_dept_id. Either column NULL = no gate on that axis.
--
-- This replaces the perm-grant mechanism for tile VIEW access. Per-user dynamic grants
-- (rbac.perm_grants, admin:system:bypass:all) no longer influence whether a tile is open.
-- They still apply to mutation/operation perms which are unchanged.
--
-- Backfill:
--   required_level    = MIN(users.staff_level among role-holders) + 1 (capped at 5)
--   required_dept_id  = most-common grantor dept, only for workflow + procurement tiles;
--                       NULL (any dept) elsewhere — HR sets per tile later.

BEGIN;

ALTER TABLE perm.tiles
  ADD COLUMN IF NOT EXISTS required_level smallint
    CHECK (required_level IS NULL OR required_level BETWEEN 1 AND 5),
  ADD COLUMN IF NOT EXISTS required_dept_id text
    REFERENCES perm.roles(id) ON DELETE RESTRICT;

CREATE OR REPLACE FUNCTION perm.tile_required_dept_is_department()
RETURNS trigger AS $$
BEGIN
  IF NEW.required_dept_id IS NOT NULL
     AND NOT EXISTS (
       SELECT 1 FROM perm.roles WHERE id = NEW.required_dept_id AND kind = 'department'
     )
  THEN
    RAISE EXCEPTION 'perm.tiles.required_dept_id (%) must reference a department-role', NEW.required_dept_id;
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS perm_tiles_required_dept_check ON perm.tiles;
CREATE TRIGGER perm_tiles_required_dept_check
  BEFORE INSERT OR UPDATE OF required_dept_id ON perm.tiles
  FOR EACH ROW EXECUTE FUNCTION perm.tile_required_dept_is_department();

-- Revert baseline view-tile grants — they no longer influence tile view.
DELETE FROM perm.role_permissions
 WHERE granted_by IN ('baseline-0028', 'baseline-0029', 'baseline-0030');

-- Backfill required_level
-- Heuristic: most senior (lowest staff_level) user holding a role that grants the perm,
-- then add 1 as a tolerance tier. Falls back to NULL if no holders have a staff_level.
WITH grant_holders AS (
  SELECT rp.permission_id,
         MIN(u.staff_level) FILTER (WHERE u.staff_level IS NOT NULL) AS min_staff_level
    FROM perm.role_permissions rp
    JOIN perm.user_roles ur ON ur.role_id = rp.role_id
    JOIN users u ON u.id = ur.user_id
   WHERE rp.effect = 'allow'
   GROUP BY rp.permission_id
)
UPDATE perm.tiles t
   SET required_level = LEAST(gh.min_staff_level + 1, 5)
  FROM grant_holders gh
 WHERE t.required_permission = gh.permission_id
   AND t.required_level IS NULL;

-- Backfill required_dept_id ONLY for workflow tiles
-- Most common dept among grant-holders; ties broken by smaller dept id.
WITH grant_dept_counts AS (
  SELECT rp.permission_id,
         u.dept_group_id,
         ROW_NUMBER() OVER (
           PARTITION BY rp.permission_id
           ORDER BY COUNT(*) DESC, u.dept_group_id
         ) AS rn
    FROM perm.role_permissions rp
    JOIN perm.user_roles ur ON ur.role_id = rp.role_id
    JOIN users u ON u.id = ur.user_id
   WHERE rp.effect = 'allow' AND u.dept_group_id IS NOT NULL
   GROUP BY rp.permission_id, u.dept_group_id
),
top_dept AS (
  SELECT permission_id, dept_group_id
    FROM grant_dept_counts WHERE rn = 1
)
UPDATE perm.tiles t
   SET required_dept_id = td.dept_group_id
  FROM top_dept td
 WHERE t.required_permission = td.permission_id
   AND t.group_name IN ('workflow','workflow-approval','workflow-procurement')
   AND t.required_dept_id IS NULL;

COMMIT;
