/** Brand read-tail r1 shared head. Text only: the two Brand management
 * authority guards render their exact original column list and predicate tail,
 * so binds, ordering and the cross-owner organization projection source stay
 * unchanged. Locks, native read ports and the cross-owner current-scope probe
 * stay untouched. */
export const brandManagementGuardHead = (columns: string, source: string, tail: string) =>
  `SELECT ${columns} FROM (${source}) organization_nodes WHERE id=? AND ${tail}`;
