import 'server-only';

/** r39 additions: byte-identical read-tail SQL sources. Each helper renders
 * exactly the literal(s) it replaces; callers keep their own binds, scope and
 * supplied statements. No r17-r38 helper, FOR SHARE/FOR UPDATE lock or spaced
 * JOIN constant is changed. */

/** Company node scope: the bare predicate SELECT. The EXISTS wrapper is added
 * by the caller; both call sites render byte-identical SQL. */
export const companyNodeScopeSource =
  "SELECT 1 FROM organization_nodes n WHERE n.id=? AND n.kind='company' AND n.company_id=? AND n.status='active'";

/** Active staff-level guard: the bare predicate SELECT. The EXISTS wrapper is
 * added by the caller; both call sites render byte-identical SQL. */
export const activeStaffLevelSource =
  'SELECT 1 FROM staff_levels WHERE id=? AND company_id=? AND active=1';

/** Node-change version guard parameterized by the caller's state clause. Each
 * call renders exactly the replaced literal; only the state clause varies. */
export const changeVersionGuardSource = (stateClause: string) =>
  `EXISTS(SELECT 1 FROM organization_node_changes WHERE id=? AND company_id=? AND version=? AND ${stateClause})`;

/** Scope-node branch-exists guard. The branch reference (bind placeholder or
 * alias column) is caller-supplied; each call renders exactly the replaced
 * literal. */
export const scopeNodeBranchExists = (branchRef: string) =>
  `EXISTS(SELECT 1 FROM organization_nodes n WHERE n.id=a.scope_node_id AND n.company_id=a.company_id AND n.branch_id=${branchRef})`;
