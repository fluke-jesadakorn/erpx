import 'server-only';

/** Owner-local Marketing read-tail head shared by the admitted list, point and
 * plain-source reads. r45: one terminal head literal; each caller supplies only
 * its owned table and exact tail, so emitted bytes stay fixed. Text only —
 * binds, row locks and emitted SQL are unchanged. Native ports and the pinned
 * settled/floor statements are not part of this family. */
export const marketingReadTailSql = (table: string, tail = ''): string =>
  `SELECT * FROM ${table}${tail}`;
