/** Procurement read-tail r19 shared fragments. Read-only text only: each builder
 * renders the exact original statement so binds, ordering, locks and writes are
 * unchanged. The r17/r18 helpers in `read-tail-sql.ts` stay untouched. */
export const tableRead = (table: string) => `SELECT * FROM ${table}`;
export const requestOrderLink = (link: string, order: string) =>
  `EXISTS(SELECT 1 FROM procurement_request_orders ${link} WHERE ${link}.request_id=? AND ${link}.order_id=${order}.id)`;
export const pointVersionGuard = (source: string, before = "", after = "") =>
  `EXISTS(SELECT 1 FROM ${source} WHERE id=? AND company_id=?${before} AND version=?${after})`;
export const receiptImage = (ref: string) =>
  `(SELECT jsonb_agg(jsonb_build_object('binding',to_jsonb(br),'receipt',to_jsonb(f),'order',to_jsonb(o)) ORDER BY br.id) FROM procurement_bill_match_receipts br JOIN commerce_fulfillments f ON f.id=br.fulfillment_id JOIN commerce_orders o ON o.id=f.order_id WHERE br.match_id=${ref})`;
