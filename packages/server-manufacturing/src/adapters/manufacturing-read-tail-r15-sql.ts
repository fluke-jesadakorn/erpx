/** Byte-identical Manufacturing read-tail r15 SQL text shared across the
 * owner-local production order readers. Text only: every caller still prepares
 * and executes its own statement with its own projection, optional join,
 * predicate, binds and result type; nothing is cached or executed here and it
 * stays outside the RD_* read registry. The `join` parameter carries the native
 * commerce output-product join verbatim where a caller needs it. */

export const manufacturingOrderJoinSelect = (columns: string, join: string, tail: string) =>
  `SELECT ${columns} FROM manufacturing_orders o JOIN manufacturing_specifications s ON s.id=o.specification_id${join} WHERE ${tail}`;
