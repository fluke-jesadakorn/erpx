/** Parameterized byte-identical select head shared by the Project visible-row
 * reads. The by-id and list readers emit this identical projection and
 * FROM/WHERE prefix; only the fixed owner-local Company/identity predicate and
 * the caller's ORDER BY stay outside the head. No SQL comes from an external
 * caller. */
export const visibleProjectRowsHead = (predicate: string) =>
  `SELECT p.* FROM staff_projects p WHERE ${predicate}`;
