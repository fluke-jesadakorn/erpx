-- Folio — deprecate approval_policies + orphan perms.
-- The workflow chain is now encoded in stage:*:act:all perms + their grants,
-- driven by STAGE_TO_PERM in lib/perm/stages.ts. The legacy approval_policies
-- JSON chain + PolicyEditor UI are removed.

BEGIN;

-- 1. Drop matched_policy_id FK columns from purchase tables.
ALTER TABLE purchase_requisitions DROP COLUMN IF EXISTS matched_policy_id;
ALTER TABLE purchase_orders       DROP COLUMN IF EXISTS matched_policy_id;

-- 2. Drop the policy table. matched_policy_id from expenses doesn't exist
--    (verified earlier), so no DROP needed there.
DROP TABLE IF EXISTS approval_policies CASCADE;

-- 3. Drop orphan policy:* permissions.
DELETE FROM perm.role_permissions WHERE permission_id LIKE 'policy:%';
DELETE FROM perm.permissions      WHERE domain = 'policy';

-- 4. Repoint the existing 'policy' tile to the rbac matrix perm. Drop the
--    now-unused tile:policy:view permission.
UPDATE perm.tiles
   SET required_permission = 'rbac:matrix:view:all',
       display_name        = 'RBAC Policy',
       subtitle            = 'Stage chain matrix - persona x stage grants'
 WHERE id = 'policy';

DELETE FROM perm.role_permissions WHERE permission_id = 'tile:policy:view:all';
DELETE FROM perm.permissions      WHERE id = 'tile:policy:view:all';

-- 5. Audit.
INSERT INTO perm.audit (kind, actor, target)
VALUES ('policy.deprecate', 'migration-0032',
        '{"note":"approval_policies dropped; chain lives in stage:*:act:all grants + STAGE_TO_PERM"}');

COMMIT;