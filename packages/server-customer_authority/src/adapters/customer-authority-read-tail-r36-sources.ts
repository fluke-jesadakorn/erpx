import 'server-only';
/** One canonical parameterized point-read projection head shared by the remaining
 *  inline single-row fact lookups (G4 read-tail r36). The caller owns the closed
 *  column list, source table, predicate and trailing clause, so every rendered
 *  statement keeps its exact pre-dedupe SQL bytes, prepared statement and binds.
 *  Pure read: no writes, no row/table locks and no advisory statements. The
 *  r21-r35 builders, portal-access row-locked reader, control-plane locks,
 *  native kinds and eligibility helpers stay untouched. */
export const pointRead = (columns: string, table: string, predicate: string, tail = '') =>
  `SELECT ${columns} FROM ${table} WHERE ${predicate}${tail}`;
