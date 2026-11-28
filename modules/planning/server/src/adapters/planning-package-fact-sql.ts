import "server-only";
/** Owner-local parameterized UNION-ALL fact branch shared by the Planning package
 * obligation read tail. Every argument is a fixed server-owned projection, source
 * or predicate fragment; no bind value or caller SQL is interpolated. Each emitted
 * branch is byte-identical to the former inline literal it replaces. */
export const planningPackageFactRows = (projection: string, source: string, predicate: string) =>
  `SELECT ${projection} FROM ${source} WHERE ${predicate}`;
