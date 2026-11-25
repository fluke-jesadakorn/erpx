import 'server-only';
/** Canonical parameterized projection-over-source probe shared by the project-worker
 * startup retained-account and terminal-placement reads. Each caller pins its exact
 * projection, owner source, alias and predicate columns; the rendered literal is
 * byte-identical to the former inline template and both binds stay at the caller.
 * Pure read only: no writes, row/table locks, advisory statements, native kinds
 * or cross-owner projections. */

export const ppRdRound6ProjectionByTwoPred = (
  projection: string,
  source: string,
  alias: string,
  column1: string,
  column2: string,
) => `SELECT ${projection} FROM (${source}) ${alias} WHERE ${alias}.${column1}=? AND ${alias}.${column2}=?`;
