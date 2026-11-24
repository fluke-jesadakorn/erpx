import 'server-only';
/** Shared owner read-tail SQL literals. Text only: callers keep their own scope/authority filters. */
export const STAFF_LEVELS_L_JOIN =
  'JOIN (SELECT "id", "company_id", "code", "name", "rank", "active", "version", "created_at", "updated_at", "level_number" FROM staff_levels) l';
export const NODE_BRANCH_ACTIVE_EXISTS =
  "EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='branch' AND status='active')";
export const NODE_BRANCH_ACTIVE =
  "SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='branch' AND status='active'";
