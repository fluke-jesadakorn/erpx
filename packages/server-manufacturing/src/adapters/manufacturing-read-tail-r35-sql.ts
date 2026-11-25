/** Byte-identical Manufacturing read-tail r35 SQL text shared by the journal
 * balance subqueries. The debit/credit line-sum witness and the production
 * cost-movement sum opened the same sum head, with only the summed column and
 * the FROM/WHERE tail varying; one parameterized emitter renders each one byte
 * for byte. Text only: every caller still prepares and executes its own
 * statement with its own binds; nothing is cached or executed here and it stays
 * outside the RD_* read registry. The r4/r11/r15-r34 helpers, all lock and
 * native read witnesses and all cross-owner statements are untouched. */

export const manufacturingSumSelect = (column: string, tail: string) =>
  `(SELECT sum(${column})${tail})`;
