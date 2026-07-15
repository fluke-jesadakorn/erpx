-- 4-segment scope grammar migration.
--
-- Order:
--   1. Add scope column (NOT NULL DEFAULT 'all') — existing rows get scope='all'
--   2. Drop FK constraints that depend on the old id
--   3. Backfill FK references to 4-segment form
--   4. Drop old PK
--   5. Replace id with generated 4-segment column
--   6. Add new composite PK + unique index on id
--   7. Re-add FK constraints
--   8. Add perm.perm_scope() helper
--   9. Seed scope variants for finance approval verbs + stage gates

BEGIN;

-- 1. Add scope column with default 'all'
ALTER TABLE perm.permissions ADD COLUMN scope text NOT NULL DEFAULT 'all'
  CHECK (scope IN ('self', 'dept', 'subtree', 'all'));

-- 2. Drop FK constraints (they target old id column)
ALTER TABLE perm.role_permissions DROP CONSTRAINT role_permissions_permission_id_fkey;
ALTER TABLE perm.tiles             DROP CONSTRAINT tiles_required_permission_fkey;
ALTER TABLE perm.acl_rules         DROP CONSTRAINT acl_rules_permission_id_fkey;

-- 3. Backfill FK references to 4-segment form (append :all where missing)
UPDATE perm.role_permissions
   SET permission_id = permission_id || ':all'
 WHERE permission_id !~ ':(self|dept|subtree|all)$';

UPDATE perm.tiles
   SET required_permission = required_permission || ':all'
 WHERE required_permission IS NOT NULL
   AND required_permission !~ ':(self|dept|subtree|all)$';

UPDATE perm.acl_rules
   SET permission_id = permission_id || ':all'
 WHERE permission_id !~ ':(self|dept|subtree|all)$';

-- 4. Drop the old PK
ALTER TABLE perm.permissions DROP CONSTRAINT permissions_pkey;

-- 5. Replace id with generated 4-segment column
ALTER TABLE perm.permissions DROP COLUMN id;
ALTER TABLE perm.permissions ADD COLUMN id text
  GENERATED ALWAYS AS (domain || ':' || subject || ':' || verb || ':' || scope) STORED;

-- 6. New composite PK + unique index on generated id
ALTER TABLE perm.permissions ADD PRIMARY KEY (domain, subject, verb, scope);
CREATE UNIQUE INDEX perm_permissions_id_idx ON perm.permissions(id);

-- 7. Re-add FK constraints (target the new unique index on id)
ALTER TABLE perm.role_permissions
  ADD CONSTRAINT role_permissions_permission_id_fkey
  FOREIGN KEY (permission_id) REFERENCES perm.permissions(id) ON DELETE CASCADE;

ALTER TABLE perm.tiles
  ADD CONSTRAINT tiles_required_permission_fkey
  FOREIGN KEY (required_permission) REFERENCES perm.permissions(id) ON DELETE RESTRICT;

ALTER TABLE perm.acl_rules
  ADD CONSTRAINT acl_rules_permission_id_fkey
  FOREIGN KEY (permission_id) REFERENCES perm.permissions(id) ON DELETE CASCADE;

-- 8. Helper: parse scope from perm key
CREATE OR REPLACE FUNCTION perm.perm_scope(perm text)
  RETURNS text LANGUAGE sql IMMUTABLE PARALLEL SAFE
  AS $$ SELECT split_part(perm, ':', 4) $$;

-- 9. Backfill scope variants for finance approval verbs (dept, subtree)
INSERT INTO perm.permissions (domain, subject, verb, scope, description)
SELECT p.domain, p.subject, p.verb, s.scope,
       COALESCE(p.description, '') || ' (scope=' || s.scope || ')'
  FROM perm.permissions p
  CROSS JOIN (VALUES ('dept'), ('subtree')) AS s(scope)
 WHERE p.scope = 'all'
   AND p.domain = 'finance'
   AND p.verb IN ('approve', 'reject', 'review')
ON CONFLICT DO NOTHING;

-- 10. Backfill scope variant for stage approval gates (dept)
INSERT INTO perm.permissions (domain, subject, verb, scope, description)
SELECT p.domain, p.subject, p.verb, s.scope,
       COALESCE(p.description, '') || ' (scope=' || s.scope || ')'
  FROM perm.permissions p
  CROSS JOIN (VALUES ('dept')) AS s(scope)
 WHERE p.scope = 'all'
   AND p.domain = 'stage'
   AND p.verb = 'act'
ON CONFLICT DO NOTHING;

COMMIT;