/** Procurement read-tail r25 shared fragments. Read-only text only: each builder
 * renders the exact original statement so binds, ordering, locks and writes are
 * unchanged. The r17/r18/r19/r20/r21/r22/r23/r24 helpers stay untouched. */
import {billMatchReceiptHead} from './read-tail-r50';
import {byIdExistsHead} from './read-tail-r52';
/** Existence scope for the actual RFQ / supplier-bill source keyed by id. */
export const sourceScopeExists = (bill: boolean, where: string) =>
  `SELECT 1 FROM ${bill ? "procurement_bill_matches" : "procurement_rfqs"} r WHERE r.id=? AND ${where}`;
/** Visible actual source guarded by the owner's company/scope predicate. The
 * byte-identical SQL head now comes from r52; every tail and bind stays here. */
export const sourceVisible = (table: string, alias: string, id: string, company: string, where: string) =>
  `${byIdExistsHead(table, alias)}${id} AND ${alias}.company_id=${company} AND ${where})`;
/** No bill-match receipt resolves to a foreign or invalid purchase. */
export const foreignBillReceiptWitness = (alias: string) =>
  `NOT EXISTS(${billMatchReceiptHead("br")} LEFT JOIN commerce_fulfillments f ON f.id=br.fulfillment_id LEFT JOIN commerce_orders o ON o.id=f.order_id WHERE br.match_id=${alias}.id AND (o.id IS NULL OR o.kind<>'purchase' OR o.company_id<>${alias}.company_id OR o.party_id<>${alias}.supplier_id OR o.currency<>${alias}.currency))`;
