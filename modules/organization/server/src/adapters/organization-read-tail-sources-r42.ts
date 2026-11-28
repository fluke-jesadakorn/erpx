import 'server-only';

/** r42 additions: byte-identical read-tail SQL sources. Each helper renders
 * exactly the literal(s) it replaces; callers keep their own binds, scope,
 * locks and supplied statements. No r17-r41 helper, FOR SHARE/FOR UPDATE lock
 * or spaced JOIN constant is changed. Only owner-supplied literals (projection
 * lists, separators, ORDER BY clauses and trailing fragments) vary, never
 * caller input. */

/** Active company node list. Family: the legacy holding-brand company scope,
 * the commercial provider listing and the people-directory company facts.
 * Each call renders exactly the replaced literal; only the projection, the
 * WHERE whitespace separator and the ORDER BY/LIMIT tail vary. */
export const activeCompanyNodes = (columns: string, whereSep: string, orderTail: string) =>
  `SELECT ${columns} FROM organization_nodes${whereSep}WHERE kind='company' AND status='active' ORDER BY ${orderTail}`;

/** Branch node lookup by company with an optional id filter. Family: the PMS
 * hospitality setup and commerce branch-node reads. Each call renders exactly
 * the replaced literal; only the projection and trailing ORDER BY vary. */
export const branchOptionalId = (columns: string, tail: string) =>
  `SELECT ${columns} FROM organization_nodes WHERE company_id=? AND kind='branch' AND status='active' AND (?::text IS NULL OR id=?)${tail}`;

/** Latest department-flow snapshot. Family: the department relocation snapshot
 * and relocation-flow guard reads. Each call renders exactly the replaced
 * literal; only the projection varies. */
export const latestDepartmentFlow = (columns: string) =>
  `SELECT ${columns} FROM department_flow_versions WHERE company_id=? AND department_id=? ORDER BY version DESC LIMIT 1`;

/** Organization change rows by the supplied predicate. Family: the change total
 * and change listing reads. Each call renders exactly the replaced literal;
 * only the projection varies and callers append their own predicate/ORDER BY. */
export const organizationChangeRows = (columns: string) =>
  `SELECT ${columns} FROM organization_node_changes WHERE `;
