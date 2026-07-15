-- 0027_drop_acl_rules.sql
-- Drop perm.acl_rules. Scope is now declarative in the 4th perm segment
-- (see lib/perm/ability.ts). Object-level conditions are derived from
-- :self / :dept / :subtree scope + the user's dept via perm.user_roles.

BEGIN;

DROP TABLE IF EXISTS perm.acl_rules CASCADE;

COMMIT;