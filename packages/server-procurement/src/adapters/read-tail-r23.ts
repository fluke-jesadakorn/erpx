/** Procurement read-tail r23 shared fragments. Read-only text only: each builder
 * renders the exact original statement so binds, ordering, locks and writes are
 * unchanged. The r17/r18/r19/r20/r21/r22 helpers stay untouched. */
/** Returned supplier-bill match with no replacement receipt for its fulfillment.
 * Read-only WHERE predicate; the alias is the bill-match source alias. */
export const returnedWithoutReplacement = (alias: string) =>
  `(${alias}.status='returned' AND NOT EXISTS( SELECT 1 FROM procurement_bill_match_receipts returned_receipt JOIN procurement_bill_match_receipts replacement_receipt ON replacement_receipt.fulfillment_id=returned_receipt.fulfillment_id AND replacement_receipt.active=TRUE WHERE returned_receipt.match_id=${alias}.id AND replacement_receipt.match_id<>${alias}.id))`;
/** Bill-match receipt binding witness; renders the exact EXISTS/NOT EXISTS text.
 * No binds, locks or writes. */
export const billReceiptBinding = (negated: boolean, inner: string, outer: string) =>
  `${negated ? "NOT " : ""}EXISTS(SELECT 1 FROM procurement_bill_match_receipts ${inner} WHERE ${inner}.match_id=${outer}.id AND ${inner}.fulfillment_id=${outer}.fulfillment_id)`;
