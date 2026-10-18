import "server-only";
import {createHash} from "node:crypto";
import type {PostgresDatabase} from "../../runtime/public/runtime/postgres";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {packageSettlementSource,type PackageSettlementSourceLock} from "../../commercial/public/package-settlement-sources";
import {manufacturingPurchaseOrderRows,manufacturingPurchaseSourceReadLocks} from "./purchases";
type Input={companyId:string;packageId:string;packageVersion:number;moduleId:string;now:number};
const sources=new WeakMap<SqlDatabase,string>();
export async function lockManufacturingPackageObligationSources(owner:PostgresDatabase,lock:PackageSettlementSourceLock){const {database,companyId}=packageSettlementSource(owner,lock);await database.batch(manufacturingPurchaseSourceReadLocks(database));await database.prepare("LOCK TABLE manufacturing_material_reservations,manufacturing_genealogy,manufacturing_cost_snapshots,manufacturing_operation_events,manufacturing_material_lot_issues,manufacturing_output_lots,manufacturing_lot_genealogy,manufacturing_inventory_moves,manufacturing_cost_pools,manufacturing_cost_allocations IN SHARE MODE").run();sources.set(database,companyId);}
export function assertManufacturingPackageObligationSources(owner:PostgresDatabase,database:SqlDatabase,companyId:string){owner.assertActiveTransaction(database);if(sources.get(database)!==companyId)throw Error("MANUFACTURING_PACKAGE_OBLIGATION_SOURCE_REQUIRED");}
/** Raw native rows include broken lineage as an obligation; a filtered purchase
 * or valid-only Work/Finance projection can never manufacture a zero. */
export const manufacturingPackageSettlementSources={
 orders:`SELECT o.*,COALESCE(valid.advanced,false) advanced,valid.id IS NOT NULL source_valid FROM manufacturing_orders o LEFT JOIN (${manufacturingPurchaseOrderRows}) valid ON valid.id=o.id AND valid.company_id=o.company_id`,
 postings:"SELECT p.*,profile.chart_id,profile.currency profile_currency,profile.wip_account_id FROM manufacturing_order_postings p LEFT JOIN manufacturing_accounting_profiles profile ON profile.id=p.profile_id AND profile.company_id=p.company_id",
 costMovements:"SELECT * FROM manufacturing_cost_movements",
 costPools:"SELECT * FROM manufacturing_cost_pools",
 costAllocations:"SELECT * FROM manufacturing_cost_allocations",
 materialReservations:"SELECT * FROM manufacturing_material_reservations",
 stockFootprint:`SELECT o.id order_id,o.company_id,o.warehouse_id,s.output_product_id product_id FROM manufacturing_orders o JOIN manufacturing_specifications s ON s.id=o.specification_id AND s.company_id=o.company_id UNION SELECT o.id,o.company_id,o.warehouse_id,line.product_id FROM manufacturing_orders o JOIN manufacturing_specification_lines line ON line.specification_id=o.specification_id`,
} as const;
export async function manufacturingPackageObligations(owner:PostgresDatabase,database:SqlDatabase,input:Input){
 assertManufacturingPackageObligationSources(owner,database,input.companyId);if(input.moduleId!=="manufacturing"||!["manufacturing","manufacturing-mrp"].includes(input.packageId)||input.packageVersion!==1||!Number.isSafeInteger(input.now)||input.now<0)throw Error("MANUFACTURING_PACKAGE_OBLIGATION_SCOPE_REQUIRED");
 const facts=(await database.prepare(`WITH scope AS(SELECT ?::text company_id,?::boolean full),orders AS MATERIALIZED(SELECT o.* FROM (${manufacturingPackageSettlementSources.orders}) o,scope WHERE o.company_id=scope.company_id AND (scope.full OR o.advanced OR NOT o.source_valid))
 SELECT 'order' kind,o.id,to_jsonb(o)::text fact FROM orders o WHERE o.status NOT IN ('completed','cancelled') OR NOT o.source_valid
 UNION ALL SELECT 'specification',s.id,to_jsonb(s)::text FROM manufacturing_specifications s,scope WHERE scope.full AND s.company_id=scope.company_id AND s.status NOT IN ('approved','rejected','retired')
 UNION ALL SELECT 'accounting-profile',p.id,to_jsonb(p)::text FROM manufacturing_accounting_profiles p,scope WHERE scope.full AND p.company_id=scope.company_id AND p.status NOT IN ('approved','rejected','retired')
 UNION ALL SELECT 'material-reservation',r.id,to_jsonb(r)::text FROM manufacturing_material_reservations r JOIN orders o ON o.id=r.order_id WHERE r.quantity_milli>r.consumed_milli+r.released_milli
 UNION ALL SELECT 'cost-pool',pool.id,to_jsonb(pool)::text FROM manufacturing_cost_pools pool,scope WHERE pool.company_id=scope.company_id AND pool.status NOT IN ('allocated','reversed') AND (scope.full OR EXISTS(SELECT 1 FROM manufacturing_cost_allocations allocation JOIN orders o ON o.id=allocation.order_id WHERE allocation.pool_id=pool.id) OR NOT EXISTS(SELECT 1 FROM manufacturing_cost_allocations allocation WHERE allocation.pool_id=pool.id))
 UNION ALL SELECT 'completed-cost',o.id,to_jsonb(o)::text FROM orders o LEFT JOIN manufacturing_cost_snapshots cost ON cost.order_id=o.id WHERE o.status='completed' AND (cost.id IS NULL OR cost.total_minor<>o.material_cost_minor+o.labor_cost_minor+o.overhead_cost_minor OR cost.output_quantity_milli IS DISTINCT FROM o.reported_output_milli)
 ORDER BY kind,id`).bind(input.companyId,input.packageId==="manufacturing").all()).results;
 return {companyId:input.companyId,packageId:input.packageId,moduleId:"manufacturing",observedAt:input.now,revision:createHash("sha256").update(JSON.stringify({companyId:input.companyId,packageVersion:input.packageVersion,facts})).digest("hex"),outstanding:facts.length};
}
