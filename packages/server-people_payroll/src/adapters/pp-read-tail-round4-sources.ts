import 'server-only';
/** Canonical parameterized by-id/by-company row probe shared by the r23
 * leave-type/engagement constants and the payroll-readiness pay-group timezone
 * read. Each caller pins its exact projection and table; the rendered literal is
 * byte-identical to the former inline copy and both binds stay at the caller.
 * Pure read only: no writes, row/table locks, advisory statements, native kinds
 * or cross-owner projections. */

export const ppRdRound4ByProjectionIdCompany = (
  projection: string,
  table: string,
) => `SELECT ${projection} FROM ${table} WHERE id=? AND company_id=?`;
