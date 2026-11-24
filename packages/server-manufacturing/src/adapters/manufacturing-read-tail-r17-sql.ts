/** Byte-identical Manufacturing read-tail r17 SQL text shared across the
 * package-settlement fact UNION branches. Text only: every branch keeps its
 * own literal columns, id/projection expressions, source rows and predicate;
 * nothing is cached or executed here and it stays outside the RD_* read
 * registry. */
export const manufacturingSettlementFactSelect = (projection: string, source: string) =>
  `SELECT ${projection} FROM ${source}`;
