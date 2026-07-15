-- permanent direct user→perm grants, filtered for currently active rows.
-- Uses statement_timestamp() (not now()) to honor the actual call site time
-- and avoid clock skew between connection-reused transactions.
CREATE OR REPLACE VIEW perm.active_user_permissions AS
  SELECT * FROM perm.user_permissions
   WHERE revoked_at IS NULL
     AND (ends_at IS NULL OR ends_at > statement_timestamp());

GRANT SELECT ON perm.active_user_permissions TO contract, n8n_user;