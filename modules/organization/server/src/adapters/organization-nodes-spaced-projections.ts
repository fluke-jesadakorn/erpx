import 'server-only';
/** Spaced-column `organization_nodes` projections shared by the owner snapshot source maps.
 * Text only: callers keep their own scope/authority filters and supplied statements. */
export const ORG_NODES_SPACED_C_JOIN =
  'JOIN (SELECT "id", "kind", "slug", "display_name", "parent_id", "company_id", "branch_id", "status", "hierarchy_revision" FROM organization_nodes) c';
export const ORG_NODES_SPACED_N_JOIN =
  'JOIN (SELECT "id", "kind", "slug", "display_name", "parent_id", "company_id", "branch_id", "status", "hierarchy_revision" FROM organization_nodes) n';
export const ORG_NODES_SPACED_B_JOIN =
  'JOIN (SELECT "id", "kind", "slug", "display_name", "parent_id", "company_id", "branch_id", "status", "hierarchy_revision" FROM organization_nodes) b';
/** Alias-parameterized spaced projections for the remaining source-map entries.
 * Each renders byte-identical SQL to the literal it replaces; only the SQL alias
 * (and FROM/JOIN keyword) varies, and both are owner-supplied literals, never
 * caller input. Text only: callers keep their own scope/authority filters and
 * supplied statements. r17-r21 constants above are left untouched. */
export const ORG_NODES_SPACED_CLAUSE = (join: 'FROM' | 'JOIN', alias: string) =>
  `${join} (SELECT "id", "kind", "slug", "display_name", "parent_id", "company_id", "branch_id", "status", "hierarchy_revision" FROM organization_nodes) ${alias}`;
export const ORG_POSITIONS_SPACED_CLAUSE = (join: 'FROM' | 'JOIN', alias: string) =>
  `${join} (SELECT "id", "company_id", "branch_id", "department_id", "title", "level_id", "reports_to_position_id", "is_ceo", "active", "version", "created_by", "created_at", "updated_at", "requirements", "roster_required" FROM organization_positions) ${alias}`;
