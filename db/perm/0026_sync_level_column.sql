-- 0026_sync_level_column.sql
-- Trigger to keep perm.roles.level in sync with level perm grants.
-- This allows the column to stay (for backward compat with 30+ queries)
-- while making it fully derived from the canonical level perms.
--
-- When a role_permissions row for a level perm is added/removed/changed,
-- recompute the role's level from its remaining level perm grants.

BEGIN;

CREATE OR REPLACE FUNCTION perm.sync_role_level_from_perms() RETURNS trigger AS $$
DECLARE
  v_role_id text;
  v_new_level smallint;
BEGIN
  -- Determine which role is affected
  v_role_id := COALESCE(NEW.role_id, OLD.role_id);

  -- Skip non-level perm rows
  IF v_role_id IS NULL THEN
    RETURN COALESCE(NEW, OLD);
  END IF;

  -- Compute level = MIN(min_level) of held level perms (5 if none)
  SELECT COALESCE(MIN(
    CAST(regexp_replace(rp.permission_id, '^rbac:level:grant:min:(\d+):all$', '\1') AS smallint)
  ), 5) INTO v_new_level
    FROM perm.role_permissions rp
   WHERE rp.role_id = v_role_id
     AND rp.effect = 'allow'
     AND rp.permission_id ~ '^rbac:level:grant:min:\d+:all$';

  UPDATE perm.roles SET level = v_new_level WHERE id = v_role_id AND level IS DISTINCT FROM v_new_level;
  RETURN COALESCE(NEW, OLD);
END $$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS perm_rp_sync_level ON perm.role_permissions;
CREATE TRIGGER perm_rp_sync_level
  AFTER INSERT OR UPDATE OR DELETE ON perm.role_permissions
  FOR EACH ROW EXECUTE FUNCTION perm.sync_role_level_from_perms();

-- One-time backfill (in case any role's level drifted from its grants)
UPDATE perm.roles r
   SET level = COALESCE((
     SELECT MIN(CAST(regexp_replace(rp.permission_id, '^rbac:level:grant:min:(\d+):all$', '\1') AS smallint))
       FROM perm.role_permissions rp
      WHERE rp.role_id = r.id AND rp.effect = 'allow'
        AND rp.permission_id ~ '^rbac:level:grant:min:\d+:all$'
   ), 5);

COMMIT;