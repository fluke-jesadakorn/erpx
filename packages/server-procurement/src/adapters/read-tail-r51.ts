/** Procurement read-tail r51 shared head. Read-only text only: this renderer owns
 * the byte-identical SQL head previously inlined by r21 (`jsonbAggBy`) and r22
 * (`jsonbAggJoinBy`), so binds, ordering, locks and writes stay unchanged. */
export const jsonbAggHead = (source: string, alias: string, order: string) =>
  `(SELECT jsonb_agg(to_jsonb(${alias}) ORDER BY ${order}) FROM ${source} ${alias} `;
