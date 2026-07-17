-- Grant folio_readonly_agent SELECT on perm.* (roles, user_roles, role_permissions,
-- user_permissions) so AI ask_sql can answer "who is the CEO/CTO/CFO?" and
-- "what permissions does role X have?" type questions.
--
-- Excluded sensitive tables:
--   perm.audit       — internal audit log
--   perm.policies    — RBAC policy AST (internal rule definitions)

GRANT USAGE ON SCHEMA perm TO folio_readonly_agent;
GRANT SELECT ON perm.roles,
                perm.user_roles,
                perm.role_permissions,
                perm.user_permissions TO folio_readonly_agent;
ALTER DEFAULT PRIVILEGES IN SCHEMA perm GRANT SELECT ON TABLES TO folio_readonly_agent;

-- Schema digest intentionally excludes:
--   perm.audit             (security log — never expose to AI)
--   perm.policies          (internal rule AST — admin-only)
--   perm.policy_decisions  (runtime decisions — internal)