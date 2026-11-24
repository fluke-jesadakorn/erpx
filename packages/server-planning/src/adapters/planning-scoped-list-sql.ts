import "server-only";
/** Owner-local parameterized row-listing SQL shared by the Planning command-source
 * read tail. Every argument is a fixed server-owned identifier, predicate or
 * ordering fragment; no bind value or caller SQL is interpolated. Each emitted
 * statement is byte-identical to the former inline literal it replaces. */
export const planningScopedBranchList = (columns: string, from: string, scope: string, branch: string, order: string) =>
  `SELECT ${columns} FROM ${from} WHERE ${scope} AND (?::text IS NULL OR ${branch}) ORDER BY ${order}`;
