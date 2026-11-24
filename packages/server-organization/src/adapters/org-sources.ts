import "server-only";

/** Canonical fixed SQL projections shared by the owner projection source maps.
 * Callers keep their own scope/authority filters and supplied transactions. */
export const ORG_NODES_SOURCE =
  "SELECT \"id\",\"kind\",\"slug\",\"display_name\",\"parent_id\",\"company_id\",\"branch_id\",\"status\",\"hierarchy_revision\" FROM organization_nodes";
export const ORG_POSITIONS_SOURCE =
  "SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"title\",\"level_id\",\"reports_to_position_id\",\"is_ceo\",\"active\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"requirements\",\"roster_required\" FROM organization_positions";
/** Canonical branch-Company read shared by the beta invitation readers.
 * Each caller keeps its own binding, scope and supplied transaction. */
export const ORG_BRANCH_COMPANY_SOURCE =
  "SELECT company_id FROM organization_nodes WHERE id=? AND kind='branch' AND status='active'";
/** Canonical Company display-name read shared by the PMS/POS owner facts.
 * Each caller keeps its own binding and supplied transaction. */
export const ORG_NODE_DISPLAY_NAME_SOURCE =
  "SELECT display_name FROM organization_nodes WHERE id=?";
