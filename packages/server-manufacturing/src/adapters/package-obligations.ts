import "server-only";
import {createHash} from "node:crypto";
import type {PostgresDatabase} from "@erpx/server-runtime/public/runtime/postgres";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {packageSettlementSource,type PackageSettlementSourceLock} from "@erpx/server-commercial/public/package-settlement-sources";
import {manufacturingPurchaseOrderRows,manufacturingPurchaseSourceReadLocks} from "@erpx/server-manufacturing/adapters/purchases";
import {manufacturingRowsSelect} from "./manufacturing-read-sql-fragments";
import {manufacturingSettlementFactSelect} from "./manufacturing-read-tail-r17-sql";
import {manufacturingStockFootprintSelect} from "./manufacturing-read-tail-r23-sql";
type Input={companyId:string;packageId:string;packageVersion:number;moduleId:string;now:number};
const sources=new WeakMap<SqlDatabase,string>();
export async function lockManufacturingPackageObligationSources(owner:PostgresDatabase,lock:PackageSettlementSourceLock){const {database,companyId}=packageSettlementSource(owner,lock);await database.batch(manufacturingPurchaseSourceReadLocks(database));await database.prepare("LOCK TABLE manufacturing_material_reservations,manufacturing_genealogy,manufacturing_cost_snapshots,manufacturing_operation_events,manufacturing_material_lot_issues,manufacturing_output_lots,manufacturing_lot_genealogy,manufacturing_inventory_moves,manufacturing_cost_pools,manufacturing_cost_allocations IN SHARE MODE").run();sources.set(database,companyId);}
export function assertManufacturingPackageObligationSources(owner:PostgresDatabase,database:SqlDatabase,companyId:string){owner.assertActiveTransaction(database);if(sources.get(database)!==companyId)throw Error("MANUFACTURING_PACKAGE_OBLIGATION_SOURCE_REQUIRED");}
/** Raw native rows include broken lineage as an obligation; a filtered purchase
 * or valid-only Work/Finance projection can never manufacture a zero. */
export const manufacturingPackageSettlementSources={
 orders:`SELECT o.*,COALESCE(valid.advanced,false) advanced,valid.id IS NOT NULL source_valid FROM manufacturing_orders o LEFT JOIN (${manufacturingPurchaseOrderRows}) valid ON valid.id=o.id AND valid.company_id=o.company_id`,
 postings:"SELECT p.*,profile.chart_id,profile.currency profile_currency,profile.wip_account_id FROM manufacturing_order_postings p LEFT JOIN manufacturing_accounting_profiles profile ON profile.id=p.profile_id AND profile.company_id=p.company_id",
 costMovements:manufacturingRowsSelect("manufacturing_cost_movements"),
 costPools:manufacturingRowsSelect("manufacturing_cost_pools"),
 costAllocations:manufacturingRowsSelect("manufacturing_cost_allocations"),
 materialReservations:manufacturingRowsSelect("manufacturing_material_reservations"),
 stockFootprint:`${manufacturingStockFootprintSelect("o.id order_id,o.company_id,o.warehouse_id,s.output_product_id product_id","manufacturing_specifications","s","s.id=o.specification_id AND s.company_id=o.company_id")} UNION ${manufacturingStockFootprintSelect("o.id,o.company_id,o.warehouse_id,line.product_id","manufacturing_specification_lines","line","line.specification_id=o.specification_id")}`,
} as const;
/** Text-only cost-allocation probe shared by the cost-pool settlement fact. The
 * two probes were byte-identical apart from the optional orders join; one
 * parameterized emitter renders each one verbatim on the caller's statement.
 * It never prepares or executes and stays outside the RD_* read registry. */
export const manufacturingCostAllocationProbe = (joined: boolean) =>
  `SELECT 1 FROM manufacturing_cost_allocations allocation${joined ? " JOIN orders o ON o.id=allocation.order_id" : ""} WHERE allocation.pool_id=pool.id`;
export async function manufacturingPackageObligations(owner:PostgresDatabase,database:SqlDatabase,input:Input){
 assertManufacturingPackageObligationSources(owner,database,input.companyId);if(input.moduleId!=="manufacturing"||!["manufacturing","manufacturing-mrp"].includes(input.packageId)||input.packageVersion!==1||!Number.isSafeInteger(input.now)||input.now<0)throw Error("MANUFACTURING_PACKAGE_OBLIGATION_SCOPE_REQUIRED");
 const facts=(await database.prepare(`WITH scope AS(SELECT ?::text company_id,?::boolean full),orders AS MATERIALIZED(SELECT o.* FROM (${manufacturingPackageSettlementSources.orders}) o,scope WHERE o.company_id=scope.company_id AND (scope.full OR o.advanced OR NOT o.source_valid))
 ${manufacturingSettlementFactSelect("'order' kind,o.id,to_jsonb(o)::text fact","orders o WHERE o.status NOT IN ('completed','cancelled') OR NOT o.source_valid")}
 UNION ALL ${manufacturingSettlementFactSelect("'specification',s.id,to_jsonb(s)::text","manufacturing_specifications s,scope WHERE scope.full AND s.company_id=scope.company_id AND s.status NOT IN ('approved','rejected','retired')")}
 UNION ALL ${manufacturingSettlementFactSelect("'accounting-profile',p.id,to_jsonb(p)::text","manufacturing_accounting_profiles p,scope WHERE scope.full AND p.company_id=scope.company_id AND p.status NOT IN ('approved','rejected','retired')")}
 UNION ALL ${manufacturingSettlementFactSelect("'material-reservation',r.id,to_jsonb(r)::text","manufacturing_material_reservations r JOIN orders o ON o.id=r.order_id WHERE r.quantity_milli>r.consumed_milli+r.released_milli")}
 UNION ALL ${manufacturingSettlementFactSelect("'cost-pool',pool.id,to_jsonb(pool)::text","manufacturing_cost_pools pool,scope WHERE pool.company_id=scope.company_id AND pool.status NOT IN ('allocated','reversed') AND (scope.full OR EXISTS(${manufacturingCostAllocationProbe(true)}) OR NOT EXISTS(${manufacturingCostAllocationProbe(false)}))")}
 UNION ALL ${manufacturingSettlementFactSelect("'completed-cost',o.id,to_jsonb(o)::text","orders o LEFT JOIN manufacturing_cost_snapshots cost ON cost.order_id=o.id WHERE o.status='completed' AND (cost.id IS NULL OR cost.total_minor<>o.material_cost_minor+o.labor_cost_minor+o.overhead_cost_minor OR cost.output_quantity_milli IS DISTINCT FROM o.reported_output_milli)")}
 ORDER BY kind,id`).bind(input.companyId,input.packageId==="manufacturing").all()).results;
 return {companyId:input.companyId,packageId:input.packageId,moduleId:"manufacturing",observedAt:input.now,revision:createHash("sha256").update(JSON.stringify({companyId:input.companyId,packageVersion:input.packageVersion,facts})).digest("hex"),outstanding:facts.length};
}
