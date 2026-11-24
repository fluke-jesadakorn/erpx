// Shared owner-local all-columns by-key family. Both Records callers render
// their exact prior SQL byte-for-byte; only the table and predicate differ.
export const selectAllWhereSql = (table: string, predicate: string) =>
  `SELECT * FROM ${table} WHERE ${predicate}`;
