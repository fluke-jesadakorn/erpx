/** Byte-identical Manufacturing read-tail r16 SQL text shared across the
 * purchased production-record readers. Text only: every caller still prepares
 * and executes its own statement with its own columns, source rows, alias,
 * predicate and binds; nothing is cached or executed here and it stays outside
 * the RD_* read registry. */
export const manufacturingPurchaseRecordSelect = (columns: string, source: string, alias: string, where: string) =>
  `SELECT ${columns} FROM (${source}) ${alias} WHERE ${where}`;
