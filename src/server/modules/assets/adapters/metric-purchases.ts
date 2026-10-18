import "server-only";
import {hasModuleAccess} from "@/data/module-access";
import {hasPermission} from "@/lib/identity_access/permissions";
import type {WorkContext} from "../../identity_access/public/authorization";
import {assignmentReadSql} from "../../identity_access/public/record-source-authority";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {ApiError} from "../../runtime/public/http";
import {assetsPackageEnforcementMode,readPurchasedAccesses} from "../../commercial/public/purchase-command-boundary";
import {packageClockSql,purchasedCapabilityPredicate} from "../../commercial/public/purchased-capability-sql";
const metrics={"active-assets":{module:"assets",table:"operational_assets",capability:"assets.register"},"asset-condition-location":{module:"assets",table:"operational_assets",capability:"assets.register"},"asset-retirements":{module:"assets",table:"operational_assets",capability:"assets.register"},"open-work-orders":{module:"maintenance",table:"maintenance_work_orders",capability:"assets.maintenance"},"blocked-maintenance":{module:"maintenance",table:"maintenance_work_orders",capability:"assets.maintenance"},"asset-downtime":{module:"maintenance",table:"maintenance_work_orders",capability:"assets.downtime"}} as const;
type AssetMetricId=keyof typeof metrics;
export async function assetMetricHistoryAccess(database:SqlDatabase,actor:WorkContext){
 const mode=await assetsPackageEnforcementMode(),access=await readPurchasedAccesses(database,mode,actor.companyId,["assets.register","assets.maintenance","assets.downtime"]);if(mode!=="enforced")return Object.fromEntries(Object.keys(metrics).map(id=>[id,true]));
 const current=hasPermission(actor,"records.read")&&!!await database.prepare(`SELECT ${assignmentReadSql("asset_metric_scope","records.read")} allowed FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) asset_metric_scope`).bind(actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId).first<boolean>("allowed");
 return Object.fromEntries(Object.entries(metrics).map(([id,definition])=>[id,current&&hasModuleAccess(actor.modules,definition.module)&&access[definition.capability].history]));
}
/** Aggregate values and paginated source facts use the same owner rows and
 * current purchase. Source callbacks recheck before reusing their SQL snapshot. */
export async function assetMetricSource(database:SqlDatabase,actor:WorkContext,id:string){
 const definition=metrics[id as AssetMetricId];if(!definition)throw new ApiError(422,"METRIC_UNSUPPORTED","This Assets metric has no owner source.");
 if(!(await assetMetricHistoryAccess(database,actor))[id])throw new ApiError(403,"METRIC_SOURCE_UNAVAILABLE","This metric requires purchased Assets history and current source authority.");
 if(await assetsPackageEnforcementMode()!=="enforced")return {sql:`SELECT * FROM ${definition.table}`,args:[] as (string|number|null)[]};
 return {sql:`WITH asset_metric_clock AS MATERIALIZED (SELECT ${packageClockSql} now_ms) SELECT asset_metric_owned.* FROM ${definition.table} asset_metric_owned CROSS JOIN asset_metric_clock WHERE ${assignmentReadSql("asset_metric_owned","records.read")} AND ${purchasedCapabilityPredicate("asset_metric_owned.company_id",definition.capability,"history","asset_metric_clock.now_ms")}`,args:[actor.assignmentId,actor.userId,actor.role]};
}
