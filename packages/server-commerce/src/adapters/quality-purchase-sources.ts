import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export type QualityHoldOrigin={id:string;companyId:string;branchId:string;warehouseId:string;productId:string;lotId:string|null;sourceType:string|null;sourceId:string|null;createdBy:string;quantityMilli:number};
export type CommerceQualityOrigin={kind:"receipt"|"customer-return";ownerId:string;sourceId:string;capabilities:readonly string[];stockCapability:"supply.stock"|"inventory.stock-core";financeCapability:"finance.inventory-valuation"|"finance.sales-posting";witness:string};
/** Closed actual owner rows, reusable by Quality's correlated history scopes.
 * The expected hold identity comes from its actual approval receipt/ordinal;
 * source labels never create this relationship. */
const receiptOrigins=`SELECT 'receipt'::text kind,a.quality_hold_id hold_id,o.company_id,o.branch_id,o.warehouse_id,ol.product_id,a.lot_id,a.id source_id,o.id owner_id,a.quantity_milli,a.value_minor,NULL::text created_by
 FROM inventory_fulfillment_lot_allocations a JOIN commerce_fulfillment_lines line ON line.id=a.fulfillment_line_id
 JOIN commerce_order_lines ol ON ol.id=line.order_line_id AND ol.item_type<>'service'
 JOIN commerce_fulfillments f ON f.id=line.fulfillment_id JOIN commerce_orders o ON o.id=f.order_id AND o.id=ol.order_id AND o.kind='purchase'
 WHERE a.quality_hold_id IS NOT NULL AND a.disposition='quarantine'`;
const returned=`FROM commerce_returns r JOIN commerce_orders o ON o.id=r.order_id AND o.company_id=r.company_id AND o.kind='sales'
 JOIN commerce_fulfillments f ON f.id=r.fulfillment_id AND f.order_id=o.id AND f.invoice_id=r.invoice_id
 JOIN commerce_return_lines line ON line.return_id=r.id JOIN commerce_fulfillment_lines fl ON fl.id=line.fulfillment_line_id AND fl.fulfillment_id=f.id
 JOIN commerce_order_lines ol ON ol.id=fl.order_line_id AND ol.order_id=o.id AND ol.product_id=line.product_id AND ol.item_type<>'service'
 JOIN command_receipts receipt ON receipt.record_id=r.id AND receipt.operation='commerce.return.approve' AND receipt.actor_id=r.approved_by AND receipt.authority_json::jsonb->>'companyId'=r.company_id`;
const untrackedReturnOrigins=`SELECT 'customer-return'::text kind,'quality-return-'||receipt.command_id||'-'||line.ordinal::text hold_id,r.company_id,r.branch_id,o.warehouse_id,line.product_id,NULL::text lot_id,line.id source_id,r.id owner_id,line.quantity_milli,line.value_minor,r.approved_by created_by ${returned}
 WHERE r.kind='customer_return' AND r.status IN ('approved','posted') AND NOT EXISTS(SELECT 1 FROM commerce_return_lot_allocations allocation WHERE allocation.return_line_id=line.id)`;
const trackedReturnOrigins=`SELECT 'customer-return'::text kind,'quality-return-'||receipt.command_id||'-'||line.ordinal::text||'-'||allocation.lot_ordinal::text hold_id,r.company_id,r.branch_id,o.warehouse_id,line.product_id,allocation.lot_id,allocation.id source_id,r.id owner_id,allocation.quantity_milli,allocation.value_minor,r.approved_by created_by ${returned}
 JOIN (SELECT *,row_number() OVER (PARTITION BY return_line_id ORDER BY id)-1 lot_ordinal FROM commerce_return_lot_allocations) allocation ON allocation.return_line_id=line.id
 JOIN inventory_fulfillment_lot_allocations original ON original.id=allocation.fulfillment_allocation_id AND original.fulfillment_line_id=fl.id AND original.lot_id=allocation.lot_id
 WHERE r.kind='customer_return' AND r.status IN ('approved','posted')`;
export const commerceQualityOriginRows=`${receiptOrigins} UNION ALL ${untrackedReturnOrigins} UNION ALL ${trackedReturnOrigins}`;
export async function commerceQualityHoldOrigin(database:SqlDatabase,hold:QualityHoldOrigin):Promise<CommerceQualityOrigin|null>{
 const kind=hold.sourceType==='goods_receipt'?'receipt':hold.sourceType==='customer_return'?'customer-return':null;if(!kind)return null;
 const row=await database.prepare(`SELECT kind,owner_id,source_id,quantity_milli,value_minor FROM (${commerceQualityOriginRows}) origin WHERE kind=? AND hold_id=? AND company_id=? AND branch_id=? AND warehouse_id=? AND product_id=? AND lot_id IS NOT DISTINCT FROM ?::text AND quantity_milli=? AND (kind='receipt' OR (source_id=? AND created_by=?))`)
  .bind(kind,hold.id,hold.companyId,hold.branchId,hold.warehouseId,hold.productId,hold.lotId,hold.quantityMilli,hold.sourceId,hold.createdBy).first<{kind:'receipt'|'customer-return';owner_id:string;source_id:string;quantity_milli:number;value_minor:number}>();
 if(!row)return null;
 return {kind:row.kind,ownerId:row.owner_id,sourceId:row.source_id,capabilities:row.kind==='receipt'?["supply.receiving","supply.stock"]:["sales.orders","inventory.stock-core"],stockCapability:row.kind==='receipt'?"supply.stock":"inventory.stock-core",financeCapability:row.kind==='receipt'?"finance.inventory-valuation":"finance.sales-posting",witness:JSON.stringify(row)};
}
/** Full owner callers acquire these before their first source snapshot. */
export function commerceQualitySourceReadLocks(database:SqlDatabase){return [database.prepare("LOCK TABLE commerce_orders,commerce_order_lines,commerce_fulfillments,commerce_fulfillment_lines,inventory_fulfillment_lot_allocations,commerce_returns,commerce_return_lines,commerce_return_lot_allocations IN SHARE MODE")];}
