import 'server-only';

/** r40 additions: byte-identical read-tail SQL sources. Each helper renders
 * exactly the literal(s) it replaces; callers keep their own binds, scope,
 * locks and supplied statements. No r17-r39 helper, FOR SHARE/FOR UPDATE lock
 * or spaced JOIN constant is changed. Only owner-supplied literals (projection
 * lists and ORDER BY clauses) vary, never caller input. */

/** Active position list by company. Family: the evaluation workspace, GPO
 * active-position and project-worker startup reads. Each call renders exactly
 * the replaced literal; only the projection and ORDER BY clause vary. */
export const activePositionList = (columns: string, orderBy: string) =>
  `SELECT ${columns} FROM organization_positions WHERE company_id=? AND active=1 ${orderBy}`;

/** Latest recommended pay default. Family: the staff-default and lifecycle
 * preview reads. Each call renders exactly the replaced literal; only the
 * projection varies. */
export const recommendedPayDefault = (projection: string) =>
  `SELECT ${projection} FROM position_pay_defaults d JOIN organization_positions p ON p.id=d.position_id WHERE d.company_id=? AND d.position_id=? AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.department_id=?) ORDER BY d.version DESC LIMIT 1`;

/** Node change by id and company. Family: the organization change read and the
 * finance attachment owner read. Each call renders exactly the replaced literal;
 * only the projection varies. */
export const nodeChangeByIdCompany = (projection: string) =>
  `SELECT ${projection} FROM organization_node_changes WHERE id=? AND company_id=?`;

/** Active staff-level name list by company. Family: the evaluation workspace
 * and project-worker startup level reads. Each call renders exactly the
 * replaced literal; only the ORDER BY clause varies. */
export const activeStaffLevels = (orderBy: string) =>
  `SELECT id,name FROM staff_levels WHERE company_id=? AND active=1 ${orderBy}`;

/** Member profile row by user. Family: the profile and profile-media reads.
 * Each call renders exactly the replaced literal; only the projection varies. */
export const memberProfileByUser = (projection: string) =>
  `SELECT ${projection} FROM member_profiles WHERE user_id=?`;
