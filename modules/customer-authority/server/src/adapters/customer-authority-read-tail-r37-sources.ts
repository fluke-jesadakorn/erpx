import 'server-only';
import {selectProjection} from './customer-authority-read-tail-r40-sources';
/** One canonical parameterized full-row projection head for the remaining
 *  reads (G4 read-tail r37). The caller owns the closed table name and optional
 *  tail clause, so every rendered statement keeps its exact pre-dedupe SQL bytes
 *  and prepared binds. The projection head comes from the r40 universal helper
 *  (G4 read-tail r43). Pure read: no writes, no row/table locks and no advisory
 *  statements. The r21-r36 builders, portal-access row-locked reader,
 *  control-plane locks, native kinds and eligibility helpers stay untouched. */
export const allColumnsProjection = (table: string, tail = '') =>
  selectProjection(`* FROM ${table}${tail}`);
