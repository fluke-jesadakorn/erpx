import 'server-only';

/** One canonical bound company-scope probe shared by the Customer Authority
 *  package-obligation scope CTE, the release-gate source and the private-file
 *  workforce-actor scope (G4 read-tail r33). Only the caller-owned trailing
 *  column list is parameterized; every rendered statement keeps its exact
 *  pre-dedupe SQL bytes and its own prepared statement and binds. This family
 *  is a pure read: no writes, no row/table locks and no advisory locks. The
 *  r21-r32 builders, the control-plane lock statements, native-owned fact kinds
 *  and the manager/eligibility helpers stay untouched. */
export const companyScopeSelect = (columns = '') => `SELECT ?::text company_id${columns}`;
