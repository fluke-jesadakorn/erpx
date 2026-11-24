import "server-only";
/** Owner-local terminal head for the Quality recursive remaining-hold lineage
 * readers. The projection and trailing JOIN/predicate fragments are fixed
 * owner-owned fragments supplied by callers; the head and spacing are emitted
 * byte-identically to the former inline copies. No caller SQL is interpolated
 * and no read here acquires a lock. */
export const qualityLineageTerminalSql = (
  columns: string,
  tail = '',
) => `SELECT ${columns} FROM lineage${tail ? ` ${tail}` : ''}`;
