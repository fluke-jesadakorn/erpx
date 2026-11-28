// Shared owner-local EXISTS single-row guard family. The three Records callers
// render their exact prior SQL byte-for-byte; only the table and predicate differ.
export const existsGuardSql = (table: string, predicate: string) =>
  `EXISTS(SELECT 1 FROM ${table} WHERE ${predicate})`;
