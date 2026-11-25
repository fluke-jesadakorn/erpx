/** r29 family: the fulfillment/order kind projection head shared by the
 * `fulfillmentKind` and `invoiceKind` entrypoint readers. The head through
 * `WHERE ` is byte-identical; only the predicate tail differs. A read-only
 * renderer: no bind hoisting, caching, lock or native change. */
export const commerceFulfillmentKindSql = (tail: string) => `SELECT o.kind FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id WHERE ${tail}`;
