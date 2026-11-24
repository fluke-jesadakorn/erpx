/** One canonical column/table-parameterized closed owner read projection shared
 *  by the Customer Authority last-audit bindings and identities sources (G4
 *  read-tail r25). The builder owns the single projection literal and renders both
 *  original projections byte-identically; only owner-supplied column lists and
 *  table names vary, never request input. Each caller keeps its own prepared
 *  statement, aliases, filters and binds. This is the single canonical
 *  projection/source head: the r34 package-obligation union fact rows
 *  render the same bytes through it (G4 read-tail r38 family). This family is a
 *  pure read: no writes, no row/table locks and no advisory locks. The r21-r24
 *  builders, source locks and native fact kinds stay untouched. */
export const ownerColumnProjection = (columns: string, table: string) =>
  `SELECT ${columns} FROM ${table}`;
