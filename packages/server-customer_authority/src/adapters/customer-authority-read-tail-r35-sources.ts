import 'server-only';
/** One canonical scalar-projection scaffold shared by
 *  the remaining inline witness reads (G4 read-tail r35). The caller owns both
 *  the scalar value and its result alias, so every rendered statement keeps its
 *  exact pre-dedupe SQL bytes, prepared statement and binds. Pure read: no
 *  writes, no source locks and no advisory statements. The r21-r34 builders,
 *  portal-access row reader, control-plane locks, native kinds and
 *  eligibility helpers stay untouched. */
export const scalarProjection = (value: string, alias: string) =>
  `SELECT ${value} ${alias}`;
