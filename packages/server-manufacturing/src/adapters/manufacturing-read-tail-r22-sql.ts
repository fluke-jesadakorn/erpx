/** Byte-identical Manufacturing read-tail r22 SQL text shared across the
 * owner-local production detail readers: material specification lines,
 * material reservations, order postings, order operations and lot genealogy.
 * Text only: every caller still prepares and executes its own statement with
 * its own projection, source rows/joins, predicate, order and binds; nothing
 * is cached or executed here and it stays outside the RD_* read registry.
 * Each argument re-emits the previously inline SQL byte for byte. The `from`
 * argument carries its own ` FROM ` separator (or the original multi-line
 * newline/indent) so single-line and multi-line callers stay byte-identical. */

export const manufacturingOrderDetailSelect = (
  projection: string,
  from: string,
  where: string,
  order: string,
) => `SELECT ${projection}${from} WHERE ${where} ORDER BY ${order}`;
