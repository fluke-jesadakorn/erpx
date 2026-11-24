import 'server-only';
/** Spaced-column `organization_nodes` projections shared by the owner snapshot source maps.
 * Text only: callers keep their own scope/authority filters and supplied statements. */
export const ORG_NODES_SPACED_N_FROM =
  'FROM (SELECT "id", "kind", "slug", "display_name", "parent_id", "company_id", "branch_id", "status", "hierarchy_revision" FROM organization_nodes) n';
export const ORG_NODES_SPACED_C_JOIN =
  'JOIN (SELECT "id", "kind", "slug", "display_name", "parent_id", "company_id", "branch_id", "status", "hierarchy_revision" FROM organization_nodes) c';
export const ORG_NODES_SPACED_C_FROM =
  'FROM (SELECT "id", "kind", "slug", "display_name", "parent_id", "company_id", "branch_id", "status", "hierarchy_revision" FROM organization_nodes) c';
export const ORG_NODES_SPACED_B_FROM =
  'FROM (SELECT "id", "kind", "slug", "display_name", "parent_id", "company_id", "branch_id", "status", "hierarchy_revision" FROM organization_nodes) b';
export const ORG_NODES_SPACED_N_JOIN =
  'JOIN (SELECT "id", "kind", "slug", "display_name", "parent_id", "company_id", "branch_id", "status", "hierarchy_revision" FROM organization_nodes) n';
export const ORG_NODES_SPACED_B_JOIN =
  'JOIN (SELECT "id", "kind", "slug", "display_name", "parent_id", "company_id", "branch_id", "status", "hierarchy_revision" FROM organization_nodes) b';
