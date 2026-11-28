/** Procurement read-tail r26 shared fragment. Read-only text only: the builder
 * renders the original statement so binds, ordering, locks and writes are
 * unchanged. The r17-r25 helpers stay untouched. */
/** Postable fulfillment lines for a set of receipt ids. `placeholders` carries
 * the caller's bound `?` list; no binds, locks or writes are introduced. */
export const fulfillmentLinesBy = (
  fulfillmentLines: string,
  orderLines: string,
  products: string,
  placeholders: string,
) =>
  `SELECT fl.*,ol.product_id,ol.description,p.sku,p.unit,p.tracking_mode FROM (${fulfillmentLines}) fl JOIN (${orderLines}) ol ON ol.id=fl.order_line_id JOIN (${products}) p ON p.id=ol.product_id WHERE fl.fulfillment_id IN (${placeholders}) ORDER BY fl.fulfillment_id,ol.ordinal`;
