import {jsonbAggHead} from './read-tail-r51';
/** Procurement read-tail r22 shared fragment. Read-only text only: each builder
 * renders the exact original statement so binds, ordering, locks and writes are
 * unchanged. The r17/r18/r19/r20/r21 helpers stay untouched; the byte-identical
 * SQL head now comes from r51. */
export const jsonbAggJoinBy = (
  source: string, alias: string, joinSource: string, joinAlias: string,
  joinOn: string, fk: string, ref: string, order: string,
) =>
  `${jsonbAggHead(source, alias, order)}JOIN ${joinSource} ${joinAlias} ON ${joinOn} WHERE ${joinAlias}.${fk}=${ref})`;
