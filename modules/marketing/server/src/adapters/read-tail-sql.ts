import 'server-only';
import { marketingSelectHeadSql } from './select-head-sql';

/** Owner-local Marketing read-tail head shared by the admitted list, point and
 * plain-source reads. r45: one terminal head literal; r64 delegates it to the
 * registered selection head. Each caller supplies only its owned table and
 * exact tail, so emitted bytes stay fixed. Text only — binds, row locks and
 * emitted SQL are unchanged. Native ports and the pinned settled/floor
 * statements are not part of this family. */
export const marketingReadTailSql = (table: string, tail = ''): string =>
  marketingSelectHeadSql('*', ` FROM ${table}${tail}`);
