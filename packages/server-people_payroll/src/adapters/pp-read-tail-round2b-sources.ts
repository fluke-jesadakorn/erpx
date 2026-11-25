import 'server-only';
/** Canonical parameterized not-exists guard shared by the r62/r63 read-tail
 * builders. The former owner-local heads are now rendered from one template;
 * each builder supplies its exact table and predicate column and passes the
 * caller's original tail through unchanged, so rendered SQL stays byte-identical.
 * Pure read only: no writes, row/table locks, advisory statements, native kinds
 * or cross-owner projections. */

export const ppRdRound2bNotExistsByOnePred = (
  table: string,
  column: string,
  tail: string,
) => `NOT EXISTS(SELECT 1 FROM ${table} WHERE ${column}=?${tail})`;
