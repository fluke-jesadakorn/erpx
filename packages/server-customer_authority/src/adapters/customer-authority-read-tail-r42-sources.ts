import 'server-only';
import {selectProjection} from './customer-authority-read-tail-r40-sources';

/** One canonical closed support-request row read head shared by the Customer
 *  Authority Portal support source reader and its thread-list reader (G4
 *  read-tail r42). The caller supplies only its owner-local column list and
 *  closed FROM/JOIN tail; each rendered read stays byte-identical to its
 *  pre-dedupe literal, including its own prepared statement, binds and source
 *  scope. The projection head comes from the r40 universal helper (G4 read-tail
 *  r43). This is pure text: no execution, writes, row/table locks, advisory
 *  locks, native statement kinds or eligibility changes. The r21-r41 builders,
 *  the table-locked control plane and all native/eligibility helpers stay
 *  untouched. */
export const supportRequestRead = (columns: string, tail: string) =>
  selectProjection(`${columns} FROM customer_portal_actions request ${tail}`);
