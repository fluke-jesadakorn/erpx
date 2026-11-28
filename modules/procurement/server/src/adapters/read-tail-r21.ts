import {jsonbAggHead} from './read-tail-r51';
/** Procurement read-tail r21 shared fragment. Read-only text only: each builder
 * renders the exact original statement so binds, ordering, locks and writes are
 * unchanged. The r17/r18/r19/r20 helpers stay untouched; the byte-identical
 * SQL head now comes from r51. */
export const jsonbAggBy = (source: string, alias: string, fk: string, ref: string, order: string) =>
  `${jsonbAggHead(source, alias, order)}WHERE ${alias}.${fk}=${ref})`;
