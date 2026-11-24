import "server-only";

/** Canonical fixed SQL projections shared by the owner projection source maps.
 * Callers keep their own scope/authority filters and supplied transactions. */
export const ORG_NODES_SOURCE =
  "SELECT \"id\",\"kind\",\"slug\",\"display_name\",\"parent_id\",\"company_id\",\"branch_id\",\"status\",\"hierarchy_revision\" FROM organization_nodes";
export const ORG_POSITIONS_SOURCE =
  "SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"title\",\"level_id\",\"reports_to_position_id\",\"is_ceo\",\"active\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"requirements\",\"roster_required\" FROM organization_positions";
