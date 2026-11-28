import 'server-only';
/** Canonical parameterized by-id/second-predicate/status existence guard shared
 * by the r19 active-engagement and r23 posted-payroll-run oracle constants. Each
 * constant pins its exact table, second predicate column and status value; the
 * rendered literal is byte-identical to the former inline copy and both binds
 * stay at the caller. Pure read only: no writes, row/table locks, advisory
 * statements, native kinds or cross-owner projections. */

export const ppRdRound5ExistsByIdAndStatus = (
  table: string,
  column2: string,
  status: string,
) => `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND ${column2}=? AND status='${status}')`;
