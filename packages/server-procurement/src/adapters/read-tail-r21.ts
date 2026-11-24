/** Procurement read-tail r21 shared fragment. Read-only text only: each builder
 * renders the exact original statement so binds, ordering, locks and writes are
 * unchanged. The r17/r18/r19/r20 helpers stay untouched. */
export const jsonbAggBy = (source: string, alias: string, fk: string, ref: string, order: string) =>
  `(SELECT jsonb_agg(to_jsonb(${alias}) ORDER BY ${order}) FROM ${source} ${alias} WHERE ${alias}.${fk}=${ref})`;
