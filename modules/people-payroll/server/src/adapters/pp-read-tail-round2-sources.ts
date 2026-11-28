import 'server-only';
/** Canonical parameterized read family for round 2 (G4). The two owner reads
 * that shared the by-id/by-company/constant predicate shape now build from one
 * template. Each caller keeps its exact projection, table, predicate column,
 * constant form and original two binds, so the rendered SQL is byte-identical.
 * Pure reads only: no writes, row/table locks, advisory statements, native
 * kinds or cross-owner projections. */

export const ppRdRound2ByIdCompanyConstant = (
  projection: string,
  table: string,
  column: string,
  value: string,
  quoted = true,
) => `SELECT ${projection} FROM ${table} WHERE id=? AND company_id=? AND ${column}=${quoted ? `'${value}'` : value}`;
