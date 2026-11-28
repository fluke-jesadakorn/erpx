/** One canonical binding/order row projection shared by the Customer Authority
 *  customer-access binding detail and workspace row reads (G4 read-tail r26).
 *  The builder owns the single projection literal and the closed binding-to-order
 *  join; each caller supplies only its owner-local projection and trailing
 *  WHERE/ORDER BY plus the closed commerce-orders source, never request input,
 *  so both render byte-identically to their pre-dedupe SQL. This family is a
 *  pure read: no writes, no row/table locks and no advisory locks. The r21-r25
 *  builders, source locks and native fact kinds stay untouched. */
export const accessBindingOrderProjection = (orders: string, projection: string, tail: string) =>
  `SELECT b.*,o.number,o.state order_state,${projection} FROM customer_access_bindings b JOIN (${orders}) o ON o.id=b.order_id AND o.company_id=b.company_id ${tail}`;
