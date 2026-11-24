import 'server-only';

/** One canonical owner existence-probe literal shared by the Customer Authority
 *  binding-identity, attachment version-guard, workforce-scope and
 *  portal-identity reads (G4 read-tail r30). Only closed owner `FROM`/`WHERE`
 *  text is supplied; each rendered statement keeps its exact pre-dedupe SQL
 *  bytes and its own prepared statement, binds and source leases. This family is
 *  a pure read: no writes, no row/table locks and no advisory locks. The r21-r29
 *  builders, control-plane lock text, native-owned statement kinds and the
 *  manager-eligibility helpers stay untouched. */
export const existenceProbe = (fromSql: string, predicate: string) =>
  `SELECT 1 ${fromSql} WHERE ${predicate}`;
