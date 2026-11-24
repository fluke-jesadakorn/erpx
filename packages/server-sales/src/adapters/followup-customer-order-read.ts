import "server-only";
/** One parameterized followup/customer order join family. The order projection,
 * column list, scope/purchase filters, predicate and tail are owner-local fixed
 * tokens composed by callers; no caller input enters the rendered statement, so
 * each call stays byte-identical to its original literal. The extra join and
 * trailing predicate/order default to empty so the work-projection head reuses
 * the same literal without a second statement. */
export const followupCustomerOrderRead = (
  orders: string,
  columns: string,
  scope: string,
  purchaseWhere: string,
  predicate = '',
  order = '',
  extraJoin = '',
) =>
  `SELECT ${columns} FROM customer_followups c JOIN (${orders}) o ON o.id=c.order_id AND o.company_id=c.company_id${extraJoin} WHERE ${scope} AND ${purchaseWhere}${predicate ? ` AND ${predicate}` : ''}${order ? ` ${order}` : ''}`;
