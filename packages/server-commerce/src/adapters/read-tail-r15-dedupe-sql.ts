/** Byte-identical SQL text shared by the round-15 Commerce read-tail families.
 * Text only: every caller still prepares and executes its own statement with its
 * own binds in its own transaction; no result is cached here. */

/** Native settlement source restricted to the settled native order ids. The
 *  default column is the settlement join; the `id` variant keeps the orders
 *  root scope byte-identical to its retired literal. */
export const COMMERCE_SETTLEMENT_BY_ORDER_SQL = (source: string, orderIds: string, column = 'order_id') =>
  `SELECT native.* FROM (${source}) native WHERE native.${column} IN (${orderIds})`;
