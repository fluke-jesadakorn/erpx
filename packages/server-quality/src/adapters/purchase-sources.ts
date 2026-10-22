import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {ApiError} from "@erpx/server-runtime/public/http";
import {commerceQualityOriginRows,commerceQualityHoldOrigin,commerceQualitySourceReadLocks} from "@erpx/server-commerce/public/quality-purchase-sources";
export type QualitySource={id:string;company_id:string;branch_id:string;warehouse_id:string;product_id:string;lot_id:string|null;source_type:string|null;source_id:string|null;spec_id:string|null;spec_version:number|null;created_by:string;held_quantity_milli:number};
export type QualityPurchaseSource={hold:QualitySource;kind:"advanced"|"receipt"|"customer-return";capabilities:readonly string[];stockCapability:"supply.stock"|"inventory.stock-core";financeCapability:"finance.inventory-valuation"|"finance.sales-posting";witness:string};
const image=(row:QualitySource)=>JSON.stringify({id:row.id,company:row.company_id,branch:row.branch_id,warehouse:row.warehouse_id,product:row.product_id,lot:row.lot_id,sourceType:row.source_type,sourceId:row.source_id,spec:row.spec_id,specVersion:row.spec_version,creator:row.created_by,quantity:row.held_quantity_milli});
/** A partial scrap's remaining hold inherits only through the posted original
 * Count, reciprocal hold/Count FKs and immutable disposition post evidence. */
export async function qualityHoldPurchaseSource(database:SqlDatabase,scope:{companyId:string;branchId:string|null},id:string):Promise<QualityPurchaseSource>{
 const hold=await database.prepare("SELECT * FROM inventory_quality_holds WHERE id=? AND company_id=? AND (?::text IS NULL OR branch_id=?)").bind(id,scope.companyId,scope.branchId,scope.branchId).first<QualitySource>();
 if(!hold)throw new ApiError(404,"QUALITY_HOLD_NOT_FOUND","Quality hold not found in the current workspace.");
 const root=await database.prepare(`WITH RECURSIVE lineage AS (
  SELECT h.*,0 depth FROM inventory_quality_holds h WHERE h.id=? AND h.company_id=?
  UNION ALL SELECT parent.*,child.depth+1 FROM lineage child
   JOIN inventory_quality_holds parent ON parent.company_id=child.company_id AND parent.branch_id=child.branch_id AND parent.warehouse_id=child.warehouse_id AND parent.product_id=child.product_id AND parent.lot_id IS NOT DISTINCT FROM child.lot_id AND parent.created_by=child.created_by AND parent.spec_id IS NOT DISTINCT FROM child.spec_id AND parent.spec_version IS NOT DISTINCT FROM child.spec_version AND parent.source_type IS NOT DISTINCT FROM child.source_type AND parent.status='disposed'
   JOIN inventory_counts c ON c.id=parent.disposition_count_id AND c.quality_hold_id=parent.id AND c.company_id=parent.company_id AND c.branch_id=parent.branch_id AND c.warehouse_id=parent.warehouse_id AND c.status='posted'
   JOIN inventory_quality_disposition_events e ON e.hold_id=parent.id AND e.count_id=c.id AND e.action='post'
   WHERE child.depth<64 AND child.id=parent.id||'-remaining-'||c.id AND child.source_id=parent.source_id||':remaining:'||c.id AND child.held_quantity_milli=parent.held_quantity_milli-parent.disposition_quantity_milli AND child.held_quantity_milli>0
 ) SELECT * FROM lineage ORDER BY depth DESC LIMIT 1`).bind(id,scope.companyId).first<QualitySource>();
 if(!root)throw new ApiError(404,"QUALITY_HOLD_NOT_FOUND","The original Quality source is unavailable.");
 if(!root.spec_id){
  const origin=await commerceQualityHoldOrigin(database,{id:root.id,companyId:root.company_id,branchId:root.branch_id,warehouseId:root.warehouse_id,productId:root.product_id,lotId:root.lot_id,sourceType:root.source_type,sourceId:root.source_id,createdBy:root.created_by,quantityMilli:Number(root.held_quantity_milli)});
  if(origin)return {hold,kind:origin.kind,capabilities:origin.capabilities,stockCapability:origin.stockCapability,financeCapability:origin.financeCapability,witness:JSON.stringify([image(hold),image(root),origin.witness])};
 }
 return {hold,kind:"advanced",capabilities:["supply.quality"],stockCapability:"supply.stock",financeCapability:"finance.inventory-valuation",witness:JSON.stringify([image(hold),image(root),"advanced"])};
}
/** Named source relation leases, before the first business snapshot. */
export function qualityPurchaseSourceReadLocks(database:SqlDatabase){return [...commerceQualitySourceReadLocks(database),database.prepare("LOCK TABLE inventory_quality_holds,inventory_quality_disposition_events,inventory_counts,inventory_count_lines,inventory_count_lot_lines IN SHARE MODE")];}

/** Fixed owner relation for before-limit shared projections and final Files
 * witnesses. The deepest proven remaining-hold ancestry selects its origin. */
const lineageParents=`JOIN inventory_quality_holds parent ON parent.company_id=child.company_id AND parent.branch_id=child.branch_id AND parent.warehouse_id=child.warehouse_id AND parent.product_id=child.product_id AND parent.lot_id IS NOT DISTINCT FROM child.lot_id AND parent.created_by=child.created_by AND parent.spec_id IS NOT DISTINCT FROM child.spec_id AND parent.spec_version IS NOT DISTINCT FROM child.spec_version AND parent.source_type IS NOT DISTINCT FROM child.source_type AND parent.status='disposed'
 JOIN inventory_counts c ON c.id=parent.disposition_count_id AND c.quality_hold_id=parent.id AND c.company_id=parent.company_id AND c.branch_id=parent.branch_id AND c.warehouse_id=parent.warehouse_id AND c.status='posted'
 JOIN inventory_quality_disposition_events e ON e.hold_id=parent.id AND e.count_id=c.id AND e.action='post'`;
const core=(alias:string)=>`jsonb_build_object('id',${alias}.id,'company',${alias}.company_id,'branch',${alias}.branch_id,'warehouse',${alias}.warehouse_id,'product',${alias}.product_id,'lot',${alias}.lot_id,'sourceType',${alias}.source_type,'sourceId',${alias}.source_id,'spec',${alias}.spec_id,'specVersion',${alias}.spec_version,'creator',${alias}.created_by,'quantity',${alias}.held_quantity_milli)`;
export const qualityPurchaseSourceRows=`WITH RECURSIVE lineage AS (
 SELECT id hold_id,id root_id,0 depth FROM inventory_quality_holds
 UNION ALL SELECT walk.hold_id,parent.id,walk.depth+1 FROM lineage walk JOIN inventory_quality_holds child ON child.id=walk.root_id ${lineageParents}
 WHERE walk.depth<64 AND child.id=parent.id||'-remaining-'||c.id AND child.source_id=parent.source_id||':remaining:'||c.id AND child.held_quantity_milli=parent.held_quantity_milli-parent.disposition_quantity_milli AND child.held_quantity_milli>0
 ), roots AS (SELECT DISTINCT ON(hold_id) hold_id,root_id FROM lineage ORDER BY hold_id,depth DESC)
 SELECT h.id,h.company_id,h.branch_id,h.warehouse_id,h.product_id,h.lot_id,root.id root_id,
 COALESCE(origin.kind,'advanced') kind,CASE WHEN origin.kind='customer-return' THEN 'inventory.stock-core' ELSE 'supply.stock' END stock_capability,
 CASE WHEN origin.kind='customer-return' THEN 'finance.sales-posting' ELSE 'finance.inventory-valuation' END finance_capability,
 jsonb_build_array(${core("h")},${core("root")},to_jsonb(origin)) witness
 FROM roots JOIN inventory_quality_holds h ON h.id=roots.hold_id JOIN inventory_quality_holds root ON root.id=roots.root_id
 LEFT JOIN (${commerceQualityOriginRows}) origin ON root.spec_id IS NULL AND origin.hold_id=root.id AND origin.company_id=root.company_id AND origin.branch_id=root.branch_id AND origin.warehouse_id=root.warehouse_id AND origin.product_id=root.product_id AND origin.lot_id IS NOT DISTINCT FROM root.lot_id AND origin.quantity_milli=root.held_quantity_milli
 AND ((origin.kind='receipt' AND root.source_type='goods_receipt') OR (origin.kind='customer-return' AND root.source_type='customer_return' AND origin.source_id=root.source_id AND origin.created_by=root.created_by))`;
