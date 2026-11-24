import 'server-only';
import {selectProjection} from './customer-authority-read-tail-r40-sources';

/** One canonical executive-evidence projection head shared by the two closed
 *  Customer Authority finance evidence reads (G4 read-tail r41). The caller
 *  supplies only its owner-local column list and closed FROM/JOIN tail; each
 *  rendered read stays byte-identical to its pre-dedupe literal, including its
 *  own prepared statement, binds and source scope. The projection head comes
 *  from the r40 universal helper (G4 read-tail r43). This is pure text: no
 *  execution, writes, row/table locks, advisory locks, native statement kinds
 *  or eligibility changes. The r21-r40 builders stay untouched. */
export const evidenceSource = (columns: string, tail: string) => selectProjection(`${columns}${tail}`);
