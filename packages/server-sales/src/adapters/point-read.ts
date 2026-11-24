import "server-only";
/** One parameterized single-key point-read family. Projection, relation, key
 * column and optional order are owner-local fixed tokens; no caller input enters
 * the rendered statement, so each call stays byte-identical to its literal. */
export const pointRead = (columns: string, table: string, column: string, order = '') =>
  `SELECT ${columns} FROM ${table} WHERE ${column}=?${order}`;
