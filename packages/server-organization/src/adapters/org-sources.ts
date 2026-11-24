import "server-only";
import {companyNodeByIdKind} from './organization-column-alias-families-r31';
import {ownerProjection} from './organization-column-alias-families-r35';

/** Canonical fixed SQL projections shared by the owner projection source maps.
 * Callers keep their own scope/authority filters and supplied transactions. */
export const ORG_NODES_SOURCE =
  ownerProjection('"id","kind","slug","display_name","parent_id","company_id","branch_id","status","hierarchy_revision"','organization_nodes');
export const ORG_POSITIONS_SOURCE =
  ownerProjection('"id","company_id","branch_id","department_id","title","level_id","reports_to_position_id","is_ceo","active","version","created_by","created_at","updated_at","requirements","roster_required"','organization_positions');
/** Canonical branch-Company read shared by the beta invitation readers.
 * Each caller keeps its own binding, scope and supplied transaction. */
export const ORG_BRANCH_COMPANY_SOURCE =
  "SELECT company_id FROM organization_nodes WHERE id=? AND kind='branch' AND status='active'";
/** Canonical Company display-name read shared by the PMS/POS owner facts.
 * Each caller keeps its own binding and supplied transaction. */
export const ORG_NODE_DISPLAY_NAME_SOURCE =
  "SELECT display_name FROM organization_nodes WHERE id=?";
/** Canonical Company-scoped display-name read shared by the finance/people owner facts.
 * Each caller keeps its own binding and supplied transaction. */
export const ORG_COMPANY_DISPLAY_NAME_SOURCE = companyNodeByIdKind('display_name');
/** Canonical active branch-list read shared by the collaboration home and beta management readers.
 * Each caller keeps its own binding and supplied transaction. */
export const ORG_ACTIVE_BRANCHES_SOURCE =
  "SELECT id,display_name FROM organization_nodes WHERE kind='branch' AND company_id=? AND status='active' AND (?::text IS NULL OR id=?)";
/** Canonical company-scoped active-id read, parameterized by table. Each caller
 * renders the exact same SQL; only the relation name varies. No caller input
 * enters the SQL text. */
export const activeIdByCompanySource = (table: string) => `SELECT id FROM ${table} WHERE company_id=? AND active=1`;
