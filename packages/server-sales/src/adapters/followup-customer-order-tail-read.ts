import "server-only";
/** Tail-parameterized variant of followupCustomerOrderRead for the followup
 * workspace list and calendar facts heads. The FROM-tail join, WHERE-tail,
 * column list, scope and order are owner-local fixed tokens composed by callers;
 * no caller input enters the rendered statement, so each call stays byte-identical
 * to its original literal. */
export const followupCustomerOrderTailRead = (
  orders: string,
  columns: string,
  scope: string,
  purchaseWhere: string,
  whereTail: string,
  order: string,
  joinTail = '',
) =>
  `SELECT ${columns} FROM customer_followups c JOIN (${orders}) o ON o.id=c.order_id AND o.company_id=c.company_id${joinTail} WHERE ${scope} AND ${purchaseWhere} AND ${whereTail} ${order}`;
