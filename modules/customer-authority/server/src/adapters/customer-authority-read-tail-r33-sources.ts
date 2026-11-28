import 'server-only';
import {selectProjection} from './customer-authority-read-tail-r40-sources';

/** One canonical bound company-scope probe shared by the Customer Authority
 *  package-obligation scope CTE, the release-gate source and the private-file
 *  workforce-actor scope (G4 read-tail r33). Only the caller-owned trailing
 *  column list is parameterized; every rendered statement keeps its exact
 *  pre-dedupe SQL bytes and its own prepared statement and binds. The projection
 *  head now comes from the r40 universal helper (G4 read-tail r43). This family
 *  is a pure read: no writes, no row/table locks and no advisory locks. The
 *  r21-r32 builders, the control-plane lock statements, native-owned fact kinds
 *  and the manager/eligibility helpers stay untouched. */
export const companyScopeSelect = (columns = '') => selectProjection(`?::text company_id${columns}`);
