import 'server-only';
/** Canonical parameterized by-id/two-predicate existence guard shared by the
 * r22 engagement-version and r23 assessment-version owner constants. Each
 * constant pins its exact table and predicate columns; the rendered literal is
 * byte-identical to the former inline copy and every bind stays at the caller.
 * Pure read only: no writes, row/table locks, advisory statements, native kinds
 * or cross-owner projections. */

export const ppRdRound3ExistsByIdTwoPred = (
  table: string,
  column2: string,
  column3: string,
) => `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND ${column2}=? AND ${column3}=?)`;
