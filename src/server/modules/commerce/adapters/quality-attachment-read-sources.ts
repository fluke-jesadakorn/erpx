import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import {commerceQualityOriginRows} from './quality-purchase-sources';

/** Quality reads can append their original access audit/source effects. Acquire
 * writer-safe leases before the first snapshot, including absent source rows. */
export function commerceQualityAttachmentReadSourceLocks(database:SqlDatabase){
 return [database.prepare('LOCK TABLE commerce_orders,commerce_order_lines,commerce_fulfillments,commerce_fulfillment_lines,inventory_fulfillment_lot_allocations,commerce_returns,commerce_return_lines,commerce_return_lot_allocations,commerce_products,commerce_warehouses,inventory_lots IN SHARE ROW EXCLUSIVE MODE')];
}

/** Closed native source image. Only an actual reciprocal Quality-origin row
 * selects the order/return; caller source labels never select another record. */
export const commerceQualityAttachmentReadSourceRows=`SELECT origin.hold_id,origin.company_id,
 jsonb_build_object('origin',to_jsonb(origin),'order',to_jsonb(o),'warehouse',to_jsonb(w),'product',to_jsonb(p),'lot',to_jsonb(l),
 'orderLines',(SELECT jsonb_agg(to_jsonb(line) ORDER BY line.id) FROM commerce_order_lines line WHERE line.order_id=o.id),
 'fulfillments',(SELECT jsonb_agg(to_jsonb(f) ORDER BY f.id) FROM commerce_fulfillments f WHERE f.order_id=o.id),
 'fulfillmentLines',(SELECT jsonb_agg(to_jsonb(line) ORDER BY line.id) FROM commerce_fulfillment_lines line JOIN commerce_fulfillments f ON f.id=line.fulfillment_id WHERE f.order_id=o.id),
 'fulfillmentLots',(SELECT jsonb_agg(to_jsonb(a) ORDER BY a.id) FROM inventory_fulfillment_lot_allocations a JOIN commerce_fulfillment_lines line ON line.id=a.fulfillment_line_id JOIN commerce_fulfillments f ON f.id=line.fulfillment_id WHERE f.order_id=o.id),
 'return',to_jsonb(r),
 'returnLines',(SELECT jsonb_agg(to_jsonb(line) ORDER BY line.id) FROM commerce_return_lines line WHERE line.return_id=r.id),
 'returnLots',(SELECT jsonb_agg(to_jsonb(a) ORDER BY a.id) FROM commerce_return_lot_allocations a JOIN commerce_return_lines line ON line.id=a.return_line_id WHERE line.return_id=r.id),
 'returnReceipt',(SELECT to_jsonb(receipt) FROM command_receipts receipt WHERE receipt.record_id=r.id AND receipt.operation='commerce.return.approve' AND receipt.actor_id=r.approved_by AND receipt.authority_json::jsonb->>'companyId'=r.company_id LIMIT 1)) image
 FROM (${commerceQualityOriginRows}) origin
 LEFT JOIN commerce_returns r ON origin.kind='customer-return' AND r.id=origin.owner_id AND r.company_id=origin.company_id
 JOIN commerce_orders o ON o.id=CASE WHEN origin.kind='receipt' THEN origin.owner_id ELSE r.order_id END AND o.company_id=origin.company_id
 LEFT JOIN commerce_warehouses w ON w.id=origin.warehouse_id AND w.company_id=origin.company_id
 LEFT JOIN commerce_products p ON p.id=origin.product_id AND p.company_id=origin.company_id
 LEFT JOIN inventory_lots l ON l.id=origin.lot_id AND l.company_id=origin.company_id`;
