import "server-only";
import {createHash} from "node:crypto";
import type {PostgresDatabase} from "@erpx/server-runtime/public/runtime/postgres";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {packageSettlementSource,type PackageSettlementSourceLock} from "@erpx/server-commercial/public/package-settlement-sources";
import {assertManufacturingPackageObligationSources,manufacturingPackageSettlementSources as manufacturing} from "@erpx/server-manufacturing/public/package-obligations";
type Input={companyId:string;packageId:string;packageVersion:number;moduleId:string;now:number};const sources=new WeakMap<SqlDatabase,string>();
export async function lockManufacturingQualityPackageObligationSources(owner:PostgresDatabase,lock:PackageSettlementSourceLock){const {database,companyId}=packageSettlementSource(owner,lock);await database.prepare("LOCK TABLE inventory_quality_holds,inventory_quality_inspections,inventory_quality_alerts,inventory_quality_disposition_events IN SHARE MODE").run();sources.set(database,companyId);}
export async function manufacturingQualityPackageObligations(owner:PostgresDatabase,database:SqlDatabase,input:Input){
 owner.assertActiveTransaction(database);if(sources.get(database)!==input.companyId)throw Error("MANUFACTURING_QUALITY_PACKAGE_OBLIGATION_SOURCE_REQUIRED");assertManufacturingPackageObligationSources(owner,database,input.companyId);if(input.moduleId!=="quality"||!["manufacturing","manufacturing-mrp"].includes(input.packageId)||input.packageVersion!==1||!Number.isSafeInteger(input.now)||input.now<0)throw Error("MANUFACTURING_QUALITY_PACKAGE_OBLIGATION_SCOPE_REQUIRED");
 // Physical overlap is deliberately conservative withdrawal evidence, never a
 // purchase grant or authority to inspect another owner's Quality record.
 const facts=(await database.prepare(`WITH scope AS(SELECT ?::text company_id,?::boolean full),orders AS MATERIALIZED(SELECT o.* FROM (${manufacturing.orders}) o,scope WHERE o.company_id=scope.company_id AND (scope.full OR o.advanced OR NOT o.source_valid)),holds AS MATERIALIZED(SELECT h.* FROM inventory_quality_holds h,scope WHERE h.company_id=scope.company_id AND EXISTS(SELECT 1 FROM (${manufacturing.stockFootprint}) source JOIN orders o ON o.id=source.order_id AND o.company_id=source.company_id WHERE source.company_id=h.company_id AND source.warehouse_id=h.warehouse_id AND source.product_id=h.product_id))
 SELECT 'quality-hold' kind,h.id,to_jsonb(h)::text fact FROM holds h WHERE h.status NOT IN ('released','cancelled','disposed')
 UNION ALL SELECT 'quality-alert',a.id,to_jsonb(a)::text FROM inventory_quality_alerts a JOIN holds h ON h.id=a.hold_id WHERE a.acknowledged_by IS NULL
 ORDER BY kind,id`).bind(input.companyId,input.packageId==="manufacturing").all()).results;
 return {companyId:input.companyId,packageId:input.packageId,moduleId:"quality",observedAt:input.now,revision:createHash("sha256").update(JSON.stringify({companyId:input.companyId,packageVersion:input.packageVersion,facts})).digest("hex"),outstanding:facts.length};
}
