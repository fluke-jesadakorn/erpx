-- Folio — index perm.roles.parent_role_id for batched chain resolver.
--
-- lib/perm/chain.ts::resolveApprovalChain will issue a single query joining
-- all in-scope stages × perm.user_roles × perm.roles × users. Without this
-- index the planner falls back to seq scan on perm.roles when stage→role
-- lookups need parent_role_id traversal.

BEGIN;

CREATE INDEX IF NOT EXISTS perm_roles_parent_idx
  ON perm.roles (parent_role_id)
  WHERE parent_role_id IS NOT NULL;

COMMIT;