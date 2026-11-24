import "server-only";
/** Owner-local parameterized single-line projection/FROM head shared by the
 * Planning command-source calendar and comparison readers. `rest` carries the
 * fixed server-owned FROM/JOIN/WHERE/ORDER fragment (no bind value or caller
 * SQL is interpolated), so each emitted statement is byte-identical to the
 * former inline literal it replaces. */
export const planningSelectColumnsFrom = (columns: string, rest: string) =>
  `SELECT ${columns} FROM ${rest}`;
