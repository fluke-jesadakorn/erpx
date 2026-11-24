/** One canonical release-gate read template shared by the Customer Authority
 *  portal release-gate aliased read and its wrapped command-assertion form
 *  (G4 read-tail r28). The Company/feature predicate, the closed scalar
 *  gate-source subselect and the trailing form (the `allowed` alias or
 *  the empty wrapped-assertion tail) are owner-owned strings; both callers render
 *  byte-identically to their pre-dedupe SQL. Callers keep their own prepared
 *  statements and binds. This family is a pure read: no writes, no row/table
 *  locks and no advisory locks. The r21-r27 builders, the gate source lock text
 *  and all native-owned statement kinds stay untouched. */
export const releaseGateRead = (predicate: string, source: string, tail: string) =>
  `SELECT ${predicate}${tail} FROM ${source}`;
