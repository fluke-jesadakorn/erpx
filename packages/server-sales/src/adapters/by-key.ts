import "server-only";
/** One byte-identical single-key read family. Only owner-local fixed relations
 * and column names are accepted; no caller input enters the rendered statement. */
export const byKey = (table: string, column: string) => `SELECT * FROM ${table} WHERE ${column}=?`;
