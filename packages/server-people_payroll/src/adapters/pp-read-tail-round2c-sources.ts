import 'server-only';
/** Canonical parameterized row-list family shared by the r57/r58/r59 read-tail
 * builders. The former owner-local heads are now rendered from one template;
 * each builder supplies its exact table and predicate column and passes the
 * caller's tail unchanged, so rendered SQL stays byte-identical. Pure read only:
 * no writes, row/table locks, advisory statements, native kinds or cross-owner
 * projections. */

export const ppRdRound2cStarByOnePred = (
  table: string,
  column: string,
  tail: string,
) => `SELECT * FROM ${table} WHERE ${column}=?${tail}`;
