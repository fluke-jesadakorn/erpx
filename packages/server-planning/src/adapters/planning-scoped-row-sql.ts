import "server-only";
/** Owner-local parameterized single-row SQL shared by the Planning command-source
 * read tail. Every argument is a fixed server-owned identifier or fragment; no
 * bind value or caller SQL is interpolated. Each emitted statement is
 * byte-identical to the former inline literal it replaces. */
export const planningScopedBranchRow = (columns: string, from: string, id: string, company: string, branch: string) =>
  `SELECT ${columns} FROM ${from} WHERE ${id}=? AND ${company}=? AND (?::text IS NULL OR ${branch}=?)`;
