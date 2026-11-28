// Shared owner-local parameterized family for the Records projection-source
// single-row reads. Both members render their exact prior SQL byte-for-byte;
// only the admitted source, alias and predicate differ. Read only: no writer,
// lock or native composition token.
export const projectionSourceGuardSql = (source: string, alias: string, predicate: string) =>
  `SELECT 1 FROM (${source}) ${alias} WHERE ${predicate}`;
