/** Owner-local parameterized statement head shared by the Quality hold-source
 * return anchors and the obligation-fact UNION arms. The projection, source
 * relation and optional predicate are fixed owner-owned fragments supplied by
 * callers; the head and spacing are emitted byte-identically to the former
 * inline copies. No arm carries a bind and no read here acquires a lock. */
export const qualitySourceArmHead = (
  columns: string,
  source: string,
  predicate = '',
) => `SELECT ${columns} FROM ${source}${predicate}`;
